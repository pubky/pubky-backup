//! Requests of a [`BackupManager`] that await the owner's approval.
//!
//! A pending request waits for its approval in a background task. Once the
//! approval arrives, the task claims the request, which takes it out of the
//! manager: from then on it can no longer be cancelled or replaced, so it is
//! never cut off half done.

use std::sync::atomic::{AtomicU64, Ordering};

use pubky::PublicKey;
use tokio::task::JoinHandle;

use super::{BackupManager, ManagerInner};

/// Source of the ids that tell pending requests apart.
static NEXT_APPROVAL_ID: AtomicU64 = AtomicU64::new(0);

/// A request that awaits approval in a background task.
pub(crate) struct PendingApproval {
    /// Tells this request apart from the ones that replace it
    pub(super) id: u64,
    /// The task waiting for the approval
    pub(super) task: JoinHandle<()>,
}

impl PendingApproval {
    pub(super) fn next_id() -> u64 {
        NEXT_APPROVAL_ID.fetch_add(1, Ordering::Relaxed)
    }

    /// Stop waiting for the approval.
    pub(super) fn cancel(self) {
        self.task.abort();
    }
}

/// Put `next` in place of the pending request in `slot`, cancelling that one.
///
/// Returns whether there was one to cancel.
pub(super) fn replace_pending_approval(
    slot: &mut Option<PendingApproval>,
    next: Option<PendingApproval>,
) -> bool {
    let replaced = std::mem::replace(slot, next);
    let had_one = replaced.is_some();
    if let Some(replaced) = replaced {
        replaced.cancel();
    }
    had_one
}

/// What a request is for, which decides where it is kept while pending.
#[derive(Clone, Copy)]
pub(super) enum ApprovalTarget<'a> {
    /// Signing in a key that is backed up, which keeps its own pending sign-in
    SignIn(&'a PublicKey),
    /// Signing in whichever key approves it, kept by the manager
    NewKeySignIn,
    /// Mirroring a key that is backed up, which keeps its own pending request
    Mirror(&'a PublicKey),
}

impl ManagerInner {
    /// Where the pending request for `target` is kept, or `None` if its key is
    /// not tracked.
    fn pending_approval(
        &mut self,
        target: ApprovalTarget<'_>,
    ) -> Option<&mut Option<PendingApproval>> {
        match target {
            ApprovalTarget::SignIn(pubky) => {
                self.keys.get_mut(pubky).map(|k| &mut k.pending_sign_in)
            }
            ApprovalTarget::NewKeySignIn => Some(&mut self.new_key_sign_in),
            ApprovalTarget::Mirror(pubky) => {
                self.keys.get_mut(pubky).map(|k| &mut k.pending_mirror)
            }
        }
    }
}

impl BackupManager {
    /// Take the pending request with this `id` out of the manager, so that it
    /// can no longer be cancelled or replaced.
    ///
    /// Returns `false` if it already was cancelled or replaced, or its key removed.
    pub(super) fn claim_approval(&self, target: ApprovalTarget<'_>, id: u64) -> bool {
        let mut inner = self.inner.write();
        let Some(slot) = inner.pending_approval(target) else {
            return false;
        };
        if slot.as_ref().is_none_or(|pending| pending.id != id) {
            return false;
        }
        // Dropping the handle leaves the claiming task running
        *slot = None;
        true
    }
}

#[cfg(test)]
pub(super) mod test_helpers {
    use super::*;

    /// A pending request whose task never finishes, as a request awaiting
    /// approval looks to the manager. Returns it with its id.
    pub(in crate::orchestrator::manager) fn pending_forever() -> (u64, PendingApproval) {
        let id = PendingApproval::next_id();
        let task = tokio::spawn(std::future::pending());
        (id, PendingApproval { id, task })
    }
}
