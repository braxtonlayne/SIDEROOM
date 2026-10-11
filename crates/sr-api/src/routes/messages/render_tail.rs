/// Attachment URLs are signed for the viewer (open question 153).
fn sign_attachments(deps: &MessagesDeps, objects: &mut [MessageObject]) {
    for o in objects {
        for a in &mut o.attachments {
            a.url = deps.sign_attachment_url(&a.url);
        }
    }
}

