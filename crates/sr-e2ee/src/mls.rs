//! MLS key agreement for calls (VOX-E2E-001, spec section 5.1, D-249), on `openmls`.
//!
//! One MLS group per call, ciphersuite `MLS_128_DHKEMP256_AES128GCM_SHA256_P256`. Handshake
//! messages are MLS PublicMessages (signed, not encrypted: they carry only public keys and
//! HPKE-encrypted path secrets), so the delivery service's external Remove proposals can be
//! processed by everyone.
//!
//! **Credentials.** A member's MLS signature key is a fresh per-session P-256 key. It is
//! certified by the account's persistent identity key (ECDSA P-256, held non-extractable in the
//! browser's WebCrypto): the identity key signs [`session_cert_message`], and the basic
//! credential carries `version 1 | user id (u64 BE) | identity public key (65-byte SEC1) |
//! signature (64-byte r||s)`. Every member checks every credential it accepts (in key packages,
//! commits and welcomes) against the user id and the identity key, so the server cannot slip
//! in a member with a key it made up for an existing user without changing that user's identity
//! key, which the pairwise privacy code (D-250) exposes.
//!
//! **Authorization.** The caller passes the *roster*: the user ids currently in the voice
//! channel, from the gateway's voice states. Adds are accepted only for roster users with a
//! valid credential, removes only for users no longer in the roster (or being re-added in the
//! same commit, a reconnect), and one leaf per user.
//!
//! **Media keys.** Each epoch, every member derives every sender's 16-byte base secret with
//! the MLS exporter: label [`crate::keys::SENDER_SECRET_LABEL`], context the sender's user id
//! as u64 little-endian ([`CallClient::sender_secrets`]).

use std::collections::HashSet;

use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use openmls_traits::types::SignatureScheme;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};

use crate::keys::{SECRET_LEN, SENDER_SECRET_LABEL, SenderSecret};

/// D-249.
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMP256_AES128GCM_SHA256_P256;
const SIG_SCHEME: SignatureScheme = SignatureScheme::ECDSA_SECP256R1_SHA256;
const CRED_VERSION: u8 = 1;
const CERT_LABEL: &[u8] = b"sr-e2ee/v1 session key";
/// Uncompressed SEC1 P-256 public key length.
pub const IDENTITY_KEY_LEN: usize = 65;
/// IEEE P1363 (r || s) ECDSA P-256 signature length, what WebCrypto produces.
pub const IDENTITY_SIG_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MlsError {
    #[error("malformed MLS message")]
    Malformed,
    #[error("bad credential")]
    BadCredential,
    #[error("not allowed: {0}")]
    NotAllowed(&'static str),
    #[error("no group")]
    NoGroup,
    #[error("wrong group or epoch")]
    WrongEpoch,
    #[error("mls: {0}")]
    Mls(String),
}

fn mls<E: std::fmt::Debug>(e: E) -> MlsError {
    MlsError::Mls(format!("{e:?}"))
}

/// What the identity key signs to certify a session's MLS signature key.
pub fn session_cert_message(user_id: u64, session_public_key: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(CERT_LABEL.len() + 8 + 2 + session_public_key.len());
    m.extend_from_slice(CERT_LABEL);
    m.extend_from_slice(&user_id.to_be_bytes());
    m.extend_from_slice(&(session_public_key.len() as u16).to_be_bytes());
    m.extend_from_slice(session_public_key);
    m
}

/// A member's verified identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub user_id: u64,
    /// 65-byte SEC1 P-256 identity public key.
    pub identity_key: Vec<u8>,
}

fn encode_credential(user_id: u64, identity_key: &[u8], sig: &[u8]) -> Vec<u8> {
    let mut c = Vec::with_capacity(1 + 8 + IDENTITY_KEY_LEN + IDENTITY_SIG_LEN);
    c.push(CRED_VERSION);
    c.extend_from_slice(&user_id.to_be_bytes());
    c.extend_from_slice(identity_key);
    c.extend_from_slice(sig);
    c
}

/// Parse and check a credential against the signature key it is bound to.
pub fn verify_credential(
    credential: &Credential,
    signature_key: &[u8],
) -> Result<Identity, MlsError> {
    let basic =
        BasicCredential::try_from(credential.clone()).map_err(|_| MlsError::BadCredential)?;
    let c = basic.identity();
    if c.len() != 1 + 8 + IDENTITY_KEY_LEN + IDENTITY_SIG_LEN || c[0] != CRED_VERSION {
        return Err(MlsError::BadCredential);
    }
    let user_id = u64::from_be_bytes(c[1..9].try_into().expect("8 bytes"));
    let ik = &c[9..9 + IDENTITY_KEY_LEN];
    let sig = &c[9 + IDENTITY_KEY_LEN..];
    let vk = VerifyingKey::from_sec1_bytes(ik).map_err(|_| MlsError::BadCredential)?;
    let sig = Signature::from_slice(sig).map_err(|_| MlsError::BadCredential)?;
    vk.verify(&session_cert_message(user_id, signature_key), &sig)
        .map_err(|_| MlsError::BadCredential)?;
    Ok(Identity {
        user_id,
        identity_key: ik.to_vec(),
    })
}

/// A session key waiting for the identity key's certificate (step 1 of [`CallClient`]).
pub struct PendingSession {
    user_id: u64,
    identity_key: Vec<u8>,
    signer: SignatureKeyPair,
}

impl PendingSession {
    /// A fresh session signature key for `user_id`, whose identity public key (65-byte SEC1) is
    /// `identity_key`.
    pub fn generate(user_id: u64, identity_key: &[u8]) -> Result<Self, MlsError> {
        VerifyingKey::from_sec1_bytes(identity_key).map_err(|_| MlsError::BadCredential)?;
        Ok(PendingSession {
            user_id,
            identity_key: identity_key.to_vec(),
            signer: SignatureKeyPair::new(SIG_SCHEME).map_err(mls)?,
        })
    }

    /// The bytes the identity key must sign (ECDSA P-256 SHA-256, r||s).
    pub fn cert_message(&self) -> Vec<u8> {
        session_cert_message(self.user_id, self.signer.public())
    }

    /// Finish with the identity key's signature over [`Self::cert_message`].
    pub fn certify(self, signature: &[u8]) -> Result<CallClient, MlsError> {
        let credential: Credential = BasicCredential::new(encode_credential(
            self.user_id,
            &self.identity_key,
            signature,
        ))
        .into();
        verify_credential(&credential, self.signer.public())?;
        let provider = OpenMlsRustCrypto::default();
        Ok(CallClient {
            credential: CredentialWithKey {
                credential,
                signature_key: self.signer.to_public_vec().into(),
            },
            signer: self.signer,
            provider,
            user_id: self.user_id,
            group: None,
        })
    }
}

/// A commit this client made, waiting for the server's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOut {
    /// The epoch the commit is based on (what op 101 `commit` sends as `epoch`).
    pub based_on: u64,
    pub commit: Vec<u8>,
    pub welcome: Option<Vec<u8>>,
    /// Users the welcome is for.
    pub welcomed: Vec<u64>,
}

/// What processing an incoming handshake message did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handled {
    /// A commit was merged; the group is at this epoch now.
    Commit { epoch: u64 },
    /// A proposal was stored for the next commit.
    Proposal,
    /// For another group or epoch (a lost race, a stale message): nothing changed.
    Ignored,
}

/// One member's MLS state for one call.
pub struct CallClient {
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    credential: CredentialWithKey,
    user_id: u64,
    group: Option<MlsGroup>,
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .use_ratchet_tree_extension(true)
        .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .build()
}

impl CallClient {
    pub fn user_id(&self) -> u64 {
        self.user_id
    }

    /// The session's MLS signature public key.
    pub fn session_public_key(&self) -> &[u8] {
        self.signer.public()
    }

    /// A key package to announce on joining (op 101 `key_package`).
    pub fn key_package(&self) -> Result<Vec<u8>, MlsError> {
        let bundle = KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential.clone(),
            )
            .map_err(mls)?;
        MlsMessageOut::from(bundle.key_package().clone())
            .tls_serialize_detached()
            .map_err(mls)
    }

    /// Start a group alone (the first one in the call), at epoch 0. `external_sender` is the
    /// delivery service's signature key and credential, allowed to propose removals.
    pub fn create_group(
        &mut self,
        group_id: &[u8],
        external_sender: Option<(&[u8], &[u8])>,
    ) -> Result<(), MlsError> {
        let mut b = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY);
        if let Some((key, identity)) = external_sender {
            let ext = Extensions::single(Extension::ExternalSenders(vec![ExternalSender::new(
                key.to_vec().into(),
                BasicCredential::new(identity.to_vec()).into(),
            )]))
            .map_err(mls)?;
            b = b.with_group_context_extensions(ext);
        }
        let group = MlsGroup::new_with_group_id(
            &self.provider,
            &self.signer,
            &b.build(),
            GroupId::from_slice(group_id),
            self.credential.clone(),
        )
        .map_err(mls)?;
        self.group = Some(group);
        Ok(())
    }

    pub fn has_group(&self) -> bool {
        self.group.as_ref().is_some_and(|g| g.is_active())
    }

    fn group(&self) -> Result<&MlsGroup, MlsError> {
        self.group
            .as_ref()
            .filter(|g| g.is_active())
            .ok_or(MlsError::NoGroup)
    }

    pub fn group_id(&self) -> Result<Vec<u8>, MlsError> {
        Ok(self.group()?.group_id().as_slice().to_vec())
    }

    pub fn epoch(&self) -> Result<u64, MlsError> {
        Ok(self.group()?.epoch().as_u64())
    }

    pub fn own_leaf(&self) -> Result<u32, MlsError> {
        Ok(self.group()?.own_leaf_index().u32())
    }

    pub fn epoch_authenticator(&self) -> Result<Vec<u8>, MlsError> {
        Ok(self.group()?.epoch_authenticator().as_slice().to_vec())
    }

    /// Members with their verified identities and leaf indexes.
    pub fn members(&self) -> Result<Vec<(u32, Identity)>, MlsError> {
        self.group()?
            .members()
            .map(|m| {
                Ok((
                    m.index.u32(),
                    verify_credential(&m.credential, &m.signature_key)?,
                ))
            })
            .collect()
    }

    /// Every member's base secret for the current epoch.
    pub fn sender_secrets(&self) -> Result<Vec<(u64, SenderSecret)>, MlsError> {
        let g = self.group()?;
        let mut out = Vec::new();
        for (_, id) in self.members()? {
            let s = g
                .export_secret(
                    self.provider.crypto(),
                    SENDER_SECRET_LABEL,
                    &id.user_id.to_le_bytes(),
                    SECRET_LEN,
                )
                .map_err(mls)?;
            out.push((
                id.user_id,
                SenderSecret::from_slice(&s).ok_or(MlsError::Malformed)?,
            ));
        }
        Ok(out)
    }

    fn parse_key_package(&self, bytes: &[u8]) -> Result<(KeyPackage, Identity), MlsError> {
        let msg = MlsMessageIn::tls_deserialize_exact(bytes).map_err(|_| MlsError::Malformed)?;
        let MlsMessageBodyIn::KeyPackage(kp) = msg.extract() else {
            return Err(MlsError::Malformed);
        };
        let kp = kp
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|_| MlsError::BadCredential)?;
        if kp.ciphersuite() != CIPHERSUITE {
            return Err(MlsError::NotAllowed("ciphersuite"));
        }
        let leaf = kp.leaf_node();
        let id = verify_credential(leaf.credential(), leaf.signature_key().as_slice())?;
        Ok((kp, id))
    }

    /// Commit adding the users behind `key_packages` (those in `roster`, with valid
    /// credentials) and removing members no longer in `roster`. A user who is already a member
    /// is replaced (a reconnect). Returns `None` when there is nothing to do. The commit is
    /// pending until [`Self::commit_accepted`] or [`Self::commit_rejected`].
    pub fn commit(
        &mut self,
        key_packages: &[Vec<u8>],
        roster: &HashSet<u64>,
    ) -> Result<Option<CommitOut>, MlsError> {
        let members = self.members()?;
        let mut adds = Vec::new();
        let mut welcomed = Vec::new();
        for b in key_packages {
            let Ok((kp, id)) = self.parse_key_package(b) else {
                continue;
            };
            if !roster.contains(&id.user_id)
                || id.user_id == self.user_id
                || welcomed.contains(&id.user_id)
            {
                continue;
            }
            welcomed.push(id.user_id);
            adds.push(kp);
        }
        let removes: Vec<LeafNodeIndex> = members
            .iter()
            .filter(|(_, id)| {
                id.user_id != self.user_id
                    && (!roster.contains(&id.user_id) || welcomed.contains(&id.user_id))
            })
            .map(|(leaf, _)| LeafNodeIndex::new(*leaf))
            .collect();
        let group = self.group.as_mut().ok_or(MlsError::NoGroup)?;
        let has_proposals = group.pending_proposals().next().is_some();
        if adds.is_empty() && removes.is_empty() && !has_proposals {
            return Ok(None);
        }
        let based_on = group.epoch().as_u64();
        let bundle = group
            .commit_builder()
            .propose_adds(adds)
            .propose_removals(removes)
            .load_psks(self.provider.storage())
            .map_err(mls)?
            .build(
                self.provider.rand(),
                self.provider.crypto(),
                &self.signer,
                |_| true,
            )
            .map_err(mls)?
            .stage_commit(&self.provider)
            .map_err(mls)?;
        let (commit, welcome, _) = bundle.into_messages();
        Ok(Some(CommitOut {
            based_on,
            commit: commit.tls_serialize_detached().map_err(mls)?,
            welcome: welcome
                .map(|w| w.tls_serialize_detached())
                .transpose()
                .map_err(mls)?,
            welcomed,
        }))
    }

    /// The server accepted our commit: apply it.
    pub fn commit_accepted(&mut self) -> Result<u64, MlsError> {
        let group = self.group.as_mut().ok_or(MlsError::NoGroup)?;
        group.merge_pending_commit(&self.provider).map_err(mls)?;
        Ok(group.epoch().as_u64())
    }

    /// Another commit won the epoch: drop ours.
    pub fn commit_rejected(&mut self) -> Result<(), MlsError> {
        if let Some(g) = self.group.as_mut() {
            g.clear_pending_commit(self.provider.storage())
                .map_err(mls)?;
        }
        Ok(())
    }

    /// Check a staged commit against the roster before merging it.
    fn authorize(&self, staged: &StagedCommit, roster: &HashSet<u64>) -> Result<(), MlsError> {
        let group = self.group()?;
        let mut added = Vec::new();
        for add in staged.add_proposals() {
            let leaf = add.add_proposal().key_package().leaf_node();
            let id = verify_credential(leaf.credential(), leaf.signature_key().as_slice())?;
            if !roster.contains(&id.user_id) || added.contains(&id.user_id) {
                return Err(MlsError::NotAllowed("add of a user not in the call"));
            }
            added.push(id.user_id);
        }
        let mut removed = Vec::new();
        for rem in staged.remove_proposals() {
            let leaf = rem.remove_proposal().removed();
            let m = group
                .member_at(leaf)
                .ok_or(MlsError::NotAllowed("remove of an empty leaf"))?;
            let id = verify_credential(&m.credential, &m.signature_key)?;
            if roster.contains(&id.user_id) && !added.contains(&id.user_id) {
                return Err(MlsError::NotAllowed("remove of a user still in the call"));
            }
            removed.push(id.user_id);
        }
        // One leaf per user: an added user must not stay a member under another leaf.
        for (_, id) in self.members()? {
            if added.contains(&id.user_id) && !removed.contains(&id.user_id) {
                return Err(MlsError::NotAllowed("second leaf for a user"));
            }
        }
        if staged.update_proposals().next().is_some() {
            return Err(MlsError::NotAllowed("update proposals"));
        }
        Ok(())
    }

    /// A commit or proposal from someone else (op 101 `commit` with another `from`, or
    /// `proposal`). Commits are checked against `roster` before they are merged.
    pub fn handle(&mut self, bytes: &[u8], roster: &HashSet<u64>) -> Result<Handled, MlsError> {
        let msg = MlsMessageIn::tls_deserialize_exact(bytes).map_err(|_| MlsError::Malformed)?;
        let proto = msg
            .try_into_protocol_message()
            .map_err(|_| MlsError::Malformed)?;
        let group = self.group.as_mut().ok_or(MlsError::NoGroup)?;
        if proto.group_id() != group.group_id() || proto.epoch() != group.epoch() {
            return Ok(Handled::Ignored);
        }
        let processed = group.process_message(&self.provider, proto).map_err(mls)?;
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                self.authorize(&staged, roster)?;
                let group = self.group.as_mut().ok_or(MlsError::NoGroup)?;
                // A commit from someone else wins over one of ours still pending.
                group
                    .clear_pending_commit(self.provider.storage())
                    .map_err(mls)?;
                group
                    .merge_staged_commit(&self.provider, *staged)
                    .map_err(mls)?;
                if !group.is_active() {
                    self.group = None;
                    return Err(MlsError::NotAllowed("removed from the group"));
                }
                Ok(Handled::Commit {
                    epoch: group.epoch().as_u64(),
                })
            }
            ProcessedMessageContent::ProposalMessage(p) => {
                // Only removals of people who left, from the delivery service or a member.
                let Proposal::Remove(r) = p.proposal() else {
                    return Err(MlsError::NotAllowed("proposal type"));
                };
                let group = self.group.as_mut().ok_or(MlsError::NoGroup)?;
                let m = group
                    .member_at(r.removed())
                    .ok_or(MlsError::NotAllowed("remove of an empty leaf"))?;
                let id = verify_credential(&m.credential, &m.signature_key)?;
                if roster.contains(&id.user_id) {
                    return Err(MlsError::NotAllowed("remove of a user still in the call"));
                }
                group
                    .store_pending_proposal(self.provider.storage(), *p)
                    .map_err(mls)?;
                Ok(Handled::Proposal)
            }
            _ => Err(MlsError::NotAllowed("message type")),
        }
    }

    /// Join from a welcome (op 101 `welcome`). Every member's credential must verify and every
    /// member must be in `roster`. Replaces any group this client had (a lost race).
    pub fn join(&mut self, welcome: &[u8], roster: &HashSet<u64>) -> Result<u64, MlsError> {
        let msg = MlsMessageIn::tls_deserialize_exact(welcome).map_err(|_| MlsError::Malformed)?;
        let MlsMessageBodyIn::Welcome(w) = msg.extract() else {
            return Err(MlsError::Malformed);
        };
        let staged = StagedWelcome::new_from_welcome(&self.provider, &join_config(), w, None)
            .map_err(mls)?;
        let group = staged.into_group(&self.provider).map_err(mls)?;
        if group.ciphersuite() != CIPHERSUITE {
            return Err(MlsError::NotAllowed("ciphersuite"));
        }
        let mut seen = HashSet::new();
        for m in group.members() {
            let id = verify_credential(&m.credential, &m.signature_key)?;
            if !roster.contains(&id.user_id) || !seen.insert(id.user_id) {
                return Err(MlsError::NotAllowed("welcome lists a user not in the call"));
            }
        }
        let epoch = group.epoch().as_u64();
        self.group = Some(group);
        Ok(epoch)
    }

    /// Forget the group (left the call).
    pub fn leave(&mut self) {
        self.group = None;
    }
}

/// The delivery service's MLS identity (VOX-E2E-004): an external sender that may propose
/// removing a member who left the call. It never joins the group and holds no group secrets.
pub struct ExternalSenderKey {
    signer: SignatureKeyPair,
    identity: Vec<u8>,
}

impl ExternalSenderKey {
    pub fn generate() -> Result<Self, MlsError> {
        Ok(ExternalSenderKey {
            signer: SignatureKeyPair::new(SIG_SCHEME).map_err(mls)?,
            identity: b"sideroom-delivery-service".to_vec(),
        })
    }

    /// Signature public key (put in the group's external senders extension).
    pub fn public_key(&self) -> &[u8] {
        self.signer.public()
    }

    /// The basic credential identity bytes.
    pub fn identity(&self) -> &[u8] {
        &self.identity
    }

    /// A signed Remove proposal for `leaf` in `group_id` at `epoch`.
    pub fn remove_proposal(
        &self,
        group_id: &[u8],
        epoch: u64,
        leaf: u32,
    ) -> Result<Vec<u8>, MlsError> {
        ExternalProposal::new_remove::<OpenMlsRustCrypto>(
            LeafNodeIndex::new(leaf),
            GroupId::from_slice(group_id),
            GroupEpoch::from(epoch),
            &self.signer,
            SenderExtensionIndex::new(0),
        )
        .map_err(mls)?
        .tls_serialize_detached()
        .map_err(mls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::SigningKey;
    use p256::ecdsa::signature::Signer as _;

    /// A user with an identity key (as the browser's WebCrypto would hold it).
    struct User {
        id: u64,
        ik: SigningKey,
    }

    impl User {
        fn new(id: u64) -> Self {
            User {
                id,
                ik: SigningKey::from_slice(&[id as u8 + 1; 32]).unwrap(),
            }
        }
        fn public(&self) -> Vec<u8> {
            self.ik
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec()
        }
        fn client(&self) -> CallClient {
            let p = PendingSession::generate(self.id, &self.public()).unwrap();
            let sig: Signature = self.ik.sign(&p.cert_message());
            p.certify(&sig.to_bytes()).unwrap()
        }
    }

    fn roster(ids: &[u64]) -> HashSet<u64> {
        ids.iter().copied().collect()
    }

    fn secrets(c: &CallClient) -> Vec<(u64, Vec<u8>)> {
        let mut v: Vec<_> = c
            .sender_secrets()
            .unwrap()
            .into_iter()
            .map(|(u, s)| (u, s.media_key(crate::frame::InstanceId([1; 4])).0.to_vec()))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn bad_certificates_are_refused() {
        let u = User::new(1);
        let p = PendingSession::generate(1, &u.public()).unwrap();
        let other = User::new(2);
        let sig: Signature = other.ik.sign(&p.cert_message());
        assert_eq!(
            p.certify(&sig.to_bytes()).err(),
            Some(MlsError::BadCredential)
        );
        assert!(PendingSession::generate(1, &[4; 10]).is_err());
    }

    #[test]
    fn add_join_secrets_and_remove() {
        let (ua, ub, uc) = (User::new(1), User::new(2), User::new(3));
        let ds = ExternalSenderKey::generate().unwrap();
        let mut a = ua.client();
        a.create_group(b"call-77", Some((ds.public_key(), ds.identity())))
            .unwrap();
        assert_eq!(a.epoch().unwrap(), 0);

        // B joins: A commits the add, the server accepts, B takes the welcome.
        let mut b = ub.client();
        let kp_b = b.key_package().unwrap();
        let r = roster(&[1, 2]);
        let c = a.commit(std::slice::from_ref(&kp_b), &r).unwrap().unwrap();
        assert_eq!((c.based_on, c.welcomed.clone()), (0, vec![2]));
        assert_eq!(a.commit_accepted().unwrap(), 1);
        assert_eq!(b.join(c.welcome.as_ref().unwrap(), &r).unwrap(), 1);
        assert_eq!(secrets(&a), secrets(&b));
        assert_eq!(
            a.epoch_authenticator().unwrap(),
            b.epoch_authenticator().unwrap()
        );
        let ids: Vec<u64> = b
            .members()
            .unwrap()
            .iter()
            .map(|(_, i)| i.user_id)
            .collect();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(b.members().unwrap()[0].1.identity_key, ua.public());

        // C joins; B commits this time, A processes it.
        let mut cc = uc.client();
        let r = roster(&[1, 2, 3]);
        let c2 = b.commit(&[cc.key_package().unwrap()], &r).unwrap().unwrap();
        assert_eq!(b.commit_accepted().unwrap(), 2);
        assert_eq!(
            a.handle(&c2.commit, &r).unwrap(),
            Handled::Commit { epoch: 2 }
        );
        assert_eq!(cc.join(c2.welcome.as_ref().unwrap(), &r).unwrap(), 2);
        assert_eq!(secrets(&a), secrets(&cc));
        let before = secrets(&a);

        // B leaves: the delivery service proposes the removal, C commits it.
        let r = roster(&[1, 3]);
        let leaf_b = b.own_leaf().unwrap();
        let prop = ds
            .remove_proposal(&a.group_id().unwrap(), 2, leaf_b)
            .unwrap();
        assert_eq!(a.handle(&prop, &r).unwrap(), Handled::Proposal);
        assert_eq!(cc.handle(&prop, &r).unwrap(), Handled::Proposal);
        let c3 = cc.commit(&[], &r).unwrap().unwrap();
        assert!(c3.welcome.is_none());
        assert_eq!(cc.commit_accepted().unwrap(), 3);
        assert_eq!(
            a.handle(&c3.commit, &r).unwrap(),
            Handled::Commit { epoch: 3 }
        );
        assert_eq!(secrets(&a), secrets(&cc));
        assert_eq!(secrets(&a).len(), 2);
        assert_ne!(secrets(&a), before);
        // B, gone, cannot follow.
        assert!(b.handle(&c3.commit, &roster(&[1, 3])).is_err() || b.epoch().unwrap() == 2);
    }

    #[test]
    fn members_refuse_unauthorized_changes() {
        let (ua, ub, uc) = (User::new(1), User::new(2), User::new(3));
        let mut a = ua.client();
        a.create_group(b"g", None).unwrap();
        let mut b = ub.client();
        let r = roster(&[1, 2]);
        let c = a.commit(&[b.key_package().unwrap()], &r).unwrap().unwrap();
        a.commit_accepted().unwrap();
        b.join(c.welcome.as_ref().unwrap(), &r).unwrap();

        // A adds C although C is not in the call (as A sees it, roster says so): B refuses.
        let cc = uc.client();
        let bad = a
            .commit(&[cc.key_package().unwrap()], &roster(&[1, 2, 3]))
            .unwrap()
            .unwrap();
        assert!(matches!(
            b.handle(&bad.commit, &r),
            Err(MlsError::NotAllowed(_))
        ));
        a.commit_rejected().unwrap();

        // A removes B although B is still in the call: B refuses.
        let kick = a.commit(&[], &roster(&[1])).unwrap().unwrap();
        assert!(matches!(
            b.handle(&kick.commit, &r),
            Err(MlsError::NotAllowed(_))
        ));
        a.commit_rejected().unwrap();

        // A key package not in the roster is skipped; nothing to do then.
        assert_eq!(a.commit(&[cc.key_package().unwrap()], &r).unwrap(), None);
        // Garbage.
        assert_eq!(b.handle(b"junk", &r), Err(MlsError::Malformed));
        assert!(b.join(b"junk", &r).is_err());
    }

    #[test]
    fn racing_commits_one_wins_and_the_loser_joins_by_welcome() {
        let (ua, ub, uc) = (User::new(1), User::new(2), User::new(3));
        let mut a = ua.client();
        a.create_group(b"g", None).unwrap();
        let mut b = ub.client();
        let r = roster(&[1, 2]);
        let c = a.commit(&[b.key_package().unwrap()], &r).unwrap().unwrap();
        a.commit_accepted().unwrap();
        b.join(c.welcome.as_ref().unwrap(), &r).unwrap();

        // C arrives; A and B both commit the add on epoch 1. The server takes A's.
        let mut cc = uc.client();
        let kp = cc.key_package().unwrap();
        let r = roster(&[1, 2, 3]);
        let ca = a.commit(std::slice::from_ref(&kp), &r).unwrap().unwrap();
        let _cb = b.commit(&[kp], &r).unwrap().unwrap();
        a.commit_accepted().unwrap();
        // B gets A's commit while its own is pending: A's wins.
        assert_eq!(
            b.handle(&ca.commit, &r).unwrap(),
            Handled::Commit { epoch: 2 }
        );
        b.commit_rejected().unwrap();
        cc.join(ca.welcome.as_ref().unwrap(), &r).unwrap();
        assert_eq!(secrets(&a), secrets(&b));
        assert_eq!(secrets(&a), secrets(&cc));
        // A stale message is ignored.
        assert_eq!(b.handle(&ca.commit, &r).unwrap(), Handled::Ignored);
    }

    #[test]
    fn a_reconnecting_user_replaces_their_old_leaf() {
        let (ua, ub) = (User::new(1), User::new(2));
        let mut a = ua.client();
        a.create_group(b"g", None).unwrap();
        let b1 = ub.client();
        let r = roster(&[1, 2]);
        a.commit(&[b1.key_package().unwrap()], &r).unwrap().unwrap();
        a.commit_accepted().unwrap();
        // B reloads: a new session, a new key package.
        let mut b2 = ub.client();
        let c = a.commit(&[b2.key_package().unwrap()], &r).unwrap().unwrap();
        a.commit_accepted().unwrap();
        b2.join(c.welcome.as_ref().unwrap(), &r).unwrap();
        assert_eq!(a.members().unwrap().len(), 2);
        assert_eq!(secrets(&a), secrets(&b2));
    }
}
