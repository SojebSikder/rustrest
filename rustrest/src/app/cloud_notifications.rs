//! Rustrest Cloud notification center: the signed-in user's inbox, fetched
//! whenever the realtime socket (re)connects and kept live by its events.

use super::Rustrest;
use crate::message::Message;
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use rustrest_cloud::wire::{Notification, NotificationPage};

/// notifications fetched and kept in memory
const PAGE_SIZE: usize = 50;

#[derive(Debug, Default)]
pub struct NotificationCenter {
    /// newest first
    pub items: Vec<Notification>,
    pub unread: usize,
    pub open: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// invite notifications whose accept/decline is in flight
    pub responding: std::collections::HashSet<String>,
}

pub fn toggle(app: &mut Rustrest) -> Task<Message> {
    let center = &mut app.cloud.notifications;
    center.open = !center.open;
    if center.open {
        return refresh(app);
    }
    Task::none()
}

pub fn close(app: &mut Rustrest) -> Task<Message> {
    app.cloud.notifications.open = false;
    Task::none()
}

/// forgets everything, on sign-out
pub fn reset(app: &mut Rustrest) {
    app.cloud.notifications = NotificationCenter::default();
}

pub fn refresh(app: &mut Rustrest) -> Task<Message> {
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    app.cloud.notifications.loading = true;

    Task::perform(
        async move { client.notifications(PAGE_SIZE).await },
        |result| Message::CloudNotificationsLoaded(result.map_err(|e| e.to_string())),
    )
}

pub fn loaded(app: &mut Rustrest, result: Result<NotificationPage, String>) -> Task<Message> {
    let center = &mut app.cloud.notifications;
    center.loading = false;
    match result {
        Ok(page) => {
            center.items = page.notifications;
            center.unread = page.unread_count;
            center.error = None;
        }
        Err(err) => center.error = Some(err),
    }
    Task::none()
}

/// a notification pushed over the realtime socket
pub fn received(app: &mut Rustrest, notification: Notification) -> Task<Message> {
    let center = &mut app.cloud.notifications;
    if center.items.iter().any(|n| n.id == notification.id) {
        return Task::none();
    }
    if !notification.is_read() {
        center.unread += 1;
    }
    let toast = (!center.open).then(|| {
        Task::done(Message::ShowToast(
            format!("{}: {}", notification.title, notification.body),
            ToastStatus::Info,
        ))
    });
    let team_related = notification.team_id().is_some();
    center.items.insert(0, notification);
    center.items.truncate(PAGE_SIZE);

    // memberships or collections changed: an open cloud modal is now stale
    let reload = (team_related && app.cloud.modal.is_some()).then(|| super::cloud::load_teams(app));
    Task::batch(toast.into_iter().chain(reload))
}

fn mark_local_read(app: &mut Rustrest, id: Option<&str>) -> Vec<String> {
    let now = chrono::Utc::now().to_rfc3339();
    let center = &mut app.cloud.notifications;
    let mut marked = Vec::new();
    for n in center.items.iter_mut() {
        if !n.is_read() && id.is_none_or(|id| n.id == id) {
            n.read_at = Some(now.clone());
            marked.push(n.id.clone());
        }
    }

    center.unread = match id {
        Some(_) => center.unread.saturating_sub(marked.len()),
        None => 0,
    };
    marked
}

pub fn mark_read(app: &mut Rustrest, id: String) -> Task<Message> {
    let marked = mark_local_read(app, Some(&id));
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    if marked.is_empty() {
        return Task::none();
    }

    Task::perform(
        async move { client.mark_notifications_read(Some(&marked)).await },
        |result| Message::CloudNotificationsSynced(result.map_err(|e| e.to_string())),
    )
}

pub fn mark_all_read(app: &mut Rustrest) -> Task<Message> {
    mark_local_read(app, None);
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move { client.mark_notifications_read(None).await },
        |result| Message::CloudNotificationsSynced(result.map_err(|e| e.to_string())),
    )
}

pub fn dismiss(app: &mut Rustrest, id: String) -> Task<Message> {
    let center = &mut app.cloud.notifications;
    if let Some(pos) = center.items.iter().position(|n| n.id == id) {
        let removed = center.items.remove(pos);
        if !removed.is_read() {
            center.unread = center.unread.saturating_sub(1);
        }
    }
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move { client.delete_notification(&id).await },
        |result| Message::CloudNotificationsSynced(result.map_err(|e| e.to_string())),
    )
}

pub fn clear(app: &mut Rustrest) -> Task<Message> {
    let center = &mut app.cloud.notifications;
    // unanswered invites stay, the server keeps them too
    center.items.retain(|n| n.invite_id().is_some());
    center.unread = center.items.iter().filter(|n| !n.is_read()).count();
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move { client.clear_notifications().await },
        |result| Message::CloudNotificationsSynced(result.map_err(|e| e.to_string())),
    )
}

/// the server's answer to a read/delete, on failure our optimistic copy is wrong, so take the server's
pub fn synced(app: &mut Rustrest, result: Result<(), String>) -> Task<Message> {
    match result {
        Ok(()) => Task::none(),
        Err(err) => Task::batch([
            Task::done(Message::ShowToast(
                format!("Couldn't update notifications: {err}"),
                ToastStatus::Error,
            )),
            refresh(app),
        ]),
    }
}

/// marks it read and, if it's about a team the user is still in, opens the
/// cloud modal on that team
pub fn pressed(app: &mut Rustrest, id: String) -> Task<Message> {
    let target = app
        .cloud
        .notifications
        .items
        .iter()
        .find(|n| n.id == id)
        .filter(|n| {
            !matches!(
                n.kind.as_str(),
                "team.deleted" | "team.member_removed" | "team.invite" | "team.invite_declined"
            )
        })
        .and_then(|n| n.team_id().map(str::to_string));

    let read = mark_read(app, id);
    let Some(team_id) = target else {
        return read;
    };

    app.cloud.notifications.open = false;
    app.cloud.last_team = Some(team_id);
    app.cloud.save_account();

    Task::batch([read, super::cloud::open_modal(app, None)])
}

/// accepts or declines the team invite behind a notification
pub fn respond_invite(app: &mut Rustrest, id: String, accept: bool) -> Task<Message> {
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    let center = &mut app.cloud.notifications;
    let Some(invite_id) = center
        .items
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.invite_id())
        .map(str::to_string)
    else {
        return Task::none();
    };
    if !center.responding.insert(id.clone()) {
        return Task::none();
    }

    Task::perform(
        async move {
            let result = client
                .respond_invite(&invite_id, accept)
                .await
                .map_err(|e| e.to_string());
            (id, result)
        },
        move |(id, result)| Message::CloudInviteResponded(id, accept, result),
    )
}

pub fn invite_responded(
    app: &mut Rustrest,
    id: String,
    accept: bool,
    result: Result<String, String>,
) -> Task<Message> {
    let center = &mut app.cloud.notifications;
    center.responding.remove(&id);
    // answered or not, the server dropped the prompt (a failed answer means
    // the invite was withdrawn or the team is gone)
    if let Some(pos) = center.items.iter().position(|n| n.id == id) {
        let removed = center.items.remove(pos);
        if !removed.is_read() {
            center.unread = center.unread.saturating_sub(1);
        }
    }

    let (toast, status) = match result {
        Ok(team) if accept => (
            format!("You joined '{team}'. Open Cloud to see its collections."),
            ToastStatus::Success,
        ),
        Ok(team) => (
            format!("Declined the invite to '{team}'"),
            ToastStatus::Info,
        ),
        Err(err) => (
            format!("Couldn't answer the invite: {err}"),
            ToastStatus::Error,
        ),
    };
    let reload = (accept && app.cloud.modal.is_some()).then(|| super::cloud::load_teams(app));
    Task::batch(
        [Task::done(Message::ShowToast(toast, status))]
            .into_iter()
            .chain(reload),
    )
}
