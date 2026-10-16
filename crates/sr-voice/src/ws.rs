//! The voice WebSocket (`GET /voice/ws`): IDENTIFY with a join token, SDP and ICE
//! signalling, heartbeats, speaking. See docs/specs/lounge-voice-video.md section 4.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use sr_voice_proto::video::Video;
use sr_voice_proto::wire::{
    Heartbeat, IceCandidate, Identify, Message, Resume, Sdp, Speaking, close, op,
};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, sleep_until, timeout};

use crate::server::{Cmd, ConnTx, Joined, SfuHandle, WsOut};

/// Time a fresh connection gets to send IDENTIFY or RESUME.
const IDENTIFY_TIMEOUT: Duration = Duration::from_secs(10);
/// The server closes after this many heartbeat intervals of silence.
const MISSED_HEARTBEATS: f64 = 2.5;
/// Client messages allowed per window (heartbeats count). Generous: push-to-talk toggles a few times a second at most.
const RATE_OPS: u32 = 200;
const RATE_WINDOW: Duration = Duration::from_secs(10);
const MAX_MESSAGE_BYTES: usize = 128 * 1024;

/// Routes to mount on the public listener: `GET /voice/ws`.
pub fn router(handle: SfuHandle) -> Router {
    Router::new()
        .route("/voice/ws", get(upgrade))
        .with_state(handle)
}

async fn upgrade(State(h): State<SfuHandle>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |sock| serve(h, sock))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn text(m: &Message) -> WsMessage {
    WsMessage::text(m.to_text())
}

fn close_frame(code: u16, reason: &str) -> WsMessage {
    WsMessage::Close(Some(CloseFrame {
        code,
        reason: reason.to_owned().into(),
    }))
}

/// Next text frame as a voice message, skipping pings. `Err(code)` is the close code to use.
async fn next_message(rx: &mut futures::stream::SplitStream<WebSocket>) -> Result<Message, u16> {
    loop {
        match rx.next().await {
            Some(Ok(WsMessage::Text(t))) => {
                return serde_json::from_str(t.as_str()).map_err(|_| close::DECODE_ERROR);
            }
            Some(Ok(WsMessage::Ping(_) | WsMessage::Pong(_))) => {}
            Some(Ok(WsMessage::Binary(_))) => return Err(close::DECODE_ERROR),
            _ => return Err(0),
        }
    }
}

async fn serve(h: SfuHandle, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();

    // ---- handshake -----------------------------------------------------------------
    let first = match timeout(IDENTIFY_TIMEOUT, next_message(&mut stream)).await {
        Ok(Ok(m)) => m,
        Ok(Err(0)) => return,
        Ok(Err(code)) => {
            let _ = sink.send(close_frame(code, "decode error")).await;
            return;
        }
        Err(_) => {
            let _ = sink
                .send(close_frame(close::NOT_AUTHENTICATED, "identify timeout"))
                .await;
            return;
        }
    };
    let conn_id = h.next_conn_id();
    let (out_tx, mut out_rx) = mpsc::channel::<WsOut>(256);
    let conn = ConnTx {
        id: conn_id,
        tx: out_tx,
    };
    let (reply_tx, reply_rx) = oneshot::channel::<Result<Joined, u16>>();
    let ready_op = match first.op {
        op::IDENTIFY => {
            let Ok(id) = first.data::<Identify>() else {
                let _ = sink
                    .send(close_frame(close::DECODE_ERROR, "bad identify"))
                    .await;
                return;
            };
            let Ok(claims) = h.signer().verify(&id.token, unix_now()) else {
                let _ = sink
                    .send(close_frame(
                        close::AUTHENTICATION_FAILED,
                        "authentication failed",
                    ))
                    .await;
                return;
            };
            if h.send(Cmd::Join {
                claims,
                conn,
                reply: reply_tx,
            })
            .await
            .is_err()
            {
                return;
            }
            op::READY
        }
        op::RESUME => {
            let Ok(r) = first.data::<Resume>() else {
                let _ = sink
                    .send(close_frame(close::DECODE_ERROR, "bad resume"))
                    .await;
                return;
            };
            if h.send(Cmd::Resume {
                session_id: r.session_id,
                resume_token: r.resume_token,
                conn,
                reply: reply_tx,
            })
            .await
            .is_err()
            {
                return;
            }
            op::RESUMED
        }
        _ => {
            let _ = sink
                .send(close_frame(close::NOT_AUTHENTICATED, "not authenticated"))
                .await;
            return;
        }
    };
    let joined = match reply_rx.await {
        Ok(Ok(j)) => j,
        Ok(Err(code)) => {
            let _ = sink.send(close_frame(code, "join refused")).await;
            return;
        }
        Err(_) => return,
    };
    let pid = joined.pid;
    if sink
        .send(text(&Message::new(ready_op, &joined.ready)))
        .await
        .is_err()
    {
        let _ = h.send(Cmd::WsLost { pid, conn_id }).await;
        return;
    }

    // ---- main loop ------------------------------------------------------------------
    let interval = h.config().heartbeat_interval;
    let silence = interval.mul_f64(MISSED_HEARTBEATS);
    let mut deadline = Instant::now() + silence;
    let (mut window_start, mut window_ops) = (Instant::now(), 0u32);
    let mut actor_closed = false;
    loop {
        tokio::select! {
            () = sleep_until(deadline) => {
                let _ = sink.send(close_frame(close::HEARTBEAT_TIMEOUT, "heartbeat timeout")).await;
                break;
            }
            out = out_rx.recv() => match out {
                Some(WsOut::Msg(m)) => {
                    if sink.send(text(&m)).await.is_err() { break; }
                }
                Some(WsOut::Close(code, why)) => {
                    let _ = sink.send(close_frame(code, why)).await;
                    actor_closed = true;
                    break;
                }
                None => { actor_closed = true; break; }
            },
            inbound = stream.next() => {
                let Some(Ok(frame)) = inbound else { break };
                let t = match frame {
                    WsMessage::Text(t) => t,
                    WsMessage::Binary(_) => {
                        let _ = sink.send(close_frame(close::DECODE_ERROR, "binary frames unsupported")).await;
                        break;
                    }
                    WsMessage::Close(_) => break,
                    _ => continue,
                };
                if window_start.elapsed() >= RATE_WINDOW {
                    (window_start, window_ops) = (Instant::now(), 0);
                }
                window_ops += 1;
                if window_ops > RATE_OPS {
                    let _ = sink.send(close_frame(close::RATE_LIMITED, "rate limited")).await;
                    break;
                }
                let Ok(m) = serde_json::from_str::<Message>(t.as_str()) else {
                    let _ = sink.send(close_frame(close::DECODE_ERROR, "decode error")).await;
                    break;
                };
                match handle_client(&h, pid, conn_id, &m).await {
                    Ok(Some(ack)) => {
                        deadline = Instant::now() + silence;
                        if sink.send(text(&ack)).await.is_err() { break; }
                    }
                    Ok(None) => {}
                    Err(code) => {
                        let _ = sink.send(close_frame(code, "protocol error")).await;
                        break;
                    }
                }
            }
        }
    }
    if !actor_closed {
        // The socket went away on its own: keep the participant for the resume window.
        let _ = h.send(Cmd::WsLost { pid, conn_id }).await;
    }
}

/// Handle one client message after the handshake. `Ok(Some(reply))` is sent straight back.
async fn handle_client(
    h: &SfuHandle,
    pid: crate::core::ParticipantId,
    conn_id: u64,
    m: &Message,
) -> Result<Option<Message>, u16> {
    let bad = |_| close::DECODE_ERROR;
    match m.op {
        op::HEARTBEAT => {
            let hb: Heartbeat = m.data().map_err(bad)?;
            Ok(Some(Message::new(op::HEARTBEAT_ACK, hb)))
        }
        op::SDP_OFFER => {
            let s: Sdp = m.data().map_err(bad)?;
            send(
                h,
                Cmd::Offer {
                    pid,
                    conn_id,
                    sdp: s.sdp,
                },
            )
            .await
        }
        op::SDP_ANSWER => {
            let s: Sdp = m.data().map_err(bad)?;
            send(
                h,
                Cmd::Answer {
                    pid,
                    conn_id,
                    sdp: s.sdp,
                },
            )
            .await
        }
        op::ICE_CANDIDATE => {
            let c: IceCandidate = m.data().map_err(bad)?;
            match c.candidate {
                Some(line) if !line.is_empty() => {
                    send(h, Cmd::Candidate { pid, conn_id, line }).await
                }
                _ => Ok(None),
            }
        }
        op::SPEAKING => {
            let s: Speaking = m.data().map_err(bad)?;
            send(
                h,
                Cmd::Speaking {
                    pid,
                    conn_id,
                    bits: s.speaking & 0b111,
                },
            )
            .await
        }
        op::IDENTIFY | op::RESUME => Err(close::ALREADY_AUTHENTICATED),
        op::VIDEO => {
            let v: Video = m.data().map_err(bad)?;
            send(
                h,
                Cmd::Video {
                    pid,
                    conn_id,
                    msg: Box::new(v),
                },
            )
            .await
        }
        // QUALITY is server to client; a client sending it is ignored, so newer clients can
        // talk to older nodes.
        op::QUALITY => Ok(None),
        _ => Err(close::UNKNOWN_OPCODE),
    }
}

async fn send(h: &SfuHandle, cmd: Cmd) -> Result<Option<Message>, u16> {
    h.send(cmd).await.map_err(|()| close::SERVER_RESTARTING)?;
    Ok(None)
}
