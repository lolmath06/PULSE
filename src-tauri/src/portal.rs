//! The XDG Desktop Portal `GlobalShortcuts` client, over D-Bus.
//!
//! The decisions and the session state machine are in
//! [`crate::overlay::global_shortcut`]; this file only talks D-Bus (through
//! `zbus`, already in the dependency graph via `tauri-plugin-opener`, with its
//! blocking API) and owns two threads:
//!
//! - a **worker** that owns the [`PortalShortcut`] state machine and makes
//!   every portal call, one at a time — binding may wait for the user to
//!   answer the desktop's dialog;
//! - a **listener** for the portal's `Activated`, `Deactivated` and
//!   `ShortcutsChanged` signals, which only reads the published session
//!   handle, so a pending dialog never delays it.
//!
//! Portal calls follow the Request pattern: subscribe to the predictable
//! `…/request/<sender>/<token>` path, call the method, wait (bounded) for its
//! `Response`. Everything here is Linux-only; other platforms get a stub that
//! reports the portal as unavailable (they never ask for it: see
//! `choose_backend`).

use std::sync::{Arc, Mutex};

use crate::overlay::global_shortcut::PortalAvailability;

/// What the portal backend currently holds, published by the worker.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortalSnapshot {
    pub session: Option<String>,
    pub requested: Option<String>,
    pub effective: Option<String>,
    pub last_error: Option<String>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub type SharedSnapshot = Arc<Mutex<PortalSnapshot>>;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn lock(snapshot: &SharedSnapshot) -> std::sync::MutexGuard<'_, PortalSnapshot> {
    snapshot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub use imp::{probe, PortalConnection, PortalService};

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::time::Duration;

    use zbus::blocking::{Connection, MessageIterator};
    use zbus::message::Type as MessageType;
    use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
    use zbus::MatchRule;

    use super::{lock, PortalAvailability, PortalSnapshot, SharedSnapshot};
    use crate::overlay::global_shortcut::{
        signal_action, BindOutcome, PortalBus, PortalError, PortalShortcut, SignalAction,
        ACTION_DESCRIPTION, ACTION_ID,
    };

    const DEST: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
    const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
    const SESSION_IFACE: &str = "org.freedesktop.portal.Session";
    const APP_ID: &str = "dev.pulse.app";

    /// `CreateSession` shows nothing; the answer is immediate.
    const CREATE_TIMEOUT: Duration = Duration::from_secs(10);
    /// `BindShortcuts` may show a dialog the user has to answer.
    const BIND_TIMEOUT: Duration = Duration::from_secs(180);
    /// How long quitting waits for the session to close.
    const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

    /// A session-bus connection on which the portal offers `GlobalShortcuts`.
    pub struct PortalConnection {
        conn: Connection,
    }

    /// Finds the portal. Called once, and only on native Wayland.
    pub fn probe() -> (PortalAvailability, Option<PortalConnection>) {
        let conn = match Connection::session() {
            Ok(conn) => conn,
            Err(error) => {
                return (
                    PortalAvailability::Unavailable {
                        reason: format!("no D-Bus session bus ({error})"),
                    },
                    None,
                )
            }
        };
        // xdg-desktop-portal ≥ 1.19 identifies an unsandboxed application by
        // this registration, which must precede any other portal call on the
        // connection. Older portals do not have it; that is not an error.
        let _ = conn.call_method(
            Some(DEST),
            PATH,
            Some("org.freedesktop.host.portal.Registry"),
            "Register",
            &(APP_ID, HashMap::<&str, Value>::new()),
        );
        let version = conn
            .call_method(
                Some(DEST),
                PATH,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(IFACE, "version"),
            )
            .and_then(|reply| reply.body().deserialize::<OwnedValue>())
            .map_err(|error| error.to_string())
            .and_then(|value| u32::try_from(value).map_err(|error| error.to_string()));
        match version {
            Ok(version) => (
                PortalAvailability::Available { version },
                Some(PortalConnection { conn }),
            ),
            Err(error) => (
                PortalAvailability::Unavailable {
                    reason: format!(
                        "this desktop's portal does not offer org.freedesktop.portal.GlobalShortcuts ({error}). \
                         GNOME provides it from GNOME 48, KDE Plasma from 5.27"
                    ),
                },
                None,
            ),
        }
    }

    /// The portal calls, over one connection.
    struct ZbusPortal {
        conn: Connection,
        sender: String,
        counter: u32,
    }

    fn inner<'a>(value: &'a Value<'a>) -> &'a Value<'a> {
        match value {
            Value::Value(boxed) => inner(boxed),
            other => other,
        }
    }

    fn text(value: &Value<'_>) -> Option<String> {
        match inner(value) {
            Value::Str(text) => Some(text.to_string()),
            Value::ObjectPath(path) => Some(path.to_string()),
            _ => None,
        }
    }

    /// Finds our action in a `shortcuts` result (`a(sa{sv})`): `None` when it
    /// is absent, `Some(trigger description)` when present.
    fn our_trigger(value: &Value<'_>) -> Option<Option<String>> {
        let Value::Array(items) = inner(value) else {
            return None;
        };
        items.iter().find_map(|item| {
            let Value::Structure(structure) = inner(item) else {
                return None;
            };
            let [id, properties] = structure.fields() else {
                return None;
            };
            if text(id).as_deref() != Some(ACTION_ID) {
                return None;
            }
            let trigger = match inner(properties) {
                Value::Dict(dict) => dict.iter().find_map(|(key, value)| {
                    (text(key).as_deref() == Some("trigger_description"))
                        .then(|| text(value))
                        .flatten()
                }),
                _ => None,
            };
            Some(trigger)
        })
    }

    fn failed(error: impl std::fmt::Display) -> PortalError {
        PortalError::Failed(error.to_string())
    }

    /// Waits for one message, at most `timeout`. The blocking iterator has no
    /// timeout of its own, so it waits on a short-lived helper thread.
    fn wait_response(
        mut responses: MessageIterator,
        timeout: Duration,
    ) -> Result<(u32, HashMap<String, OwnedValue>), PortalError> {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("pulse-portal-response".into())
            .spawn(move || {
                let result = match responses.next() {
                    Some(Ok(message)) => message
                        .body()
                        .deserialize::<(u32, HashMap<String, OwnedValue>)>()
                        .map_err(failed),
                    Some(Err(error)) => Err(failed(error)),
                    None => Err(failed("the portal connection closed")),
                };
                let _ = tx.send(result);
            })
            .map_err(failed)?;
        rx.recv_timeout(timeout).unwrap_or_else(|_| {
            Err(PortalError::Failed(format!(
                "no answer from the desktop within {} s",
                timeout.as_secs()
            )))
        })
    }

    impl ZbusPortal {
        fn new(conn: Connection) -> Self {
            let sender = conn
                .unique_name()
                .map(|name| name.trim_start_matches(':').replace('.', "_"))
                .unwrap_or_default();
            Self {
                conn,
                sender,
                counter: 0,
            }
        }

        fn token(&mut self) -> String {
            self.counter += 1;
            format!("pulse{}_{}", std::process::id(), self.counter)
        }

        fn responses(&self, path: &str) -> Result<MessageIterator, PortalError> {
            let rule = MatchRule::builder()
                .msg_type(MessageType::Signal)
                .interface(REQUEST_IFACE)
                .and_then(|builder| builder.member("Response"))
                .and_then(|builder| builder.path(path))
                .map_err(failed)?
                .build();
            MessageIterator::for_match_rule(rule, &self.conn, Some(4)).map_err(failed)
        }

        /// One portal request: subscribe, call, wait for the `Response`.
        fn request<B>(
            &mut self,
            method: &str,
            token: &str,
            body: &B,
            timeout: Duration,
        ) -> Result<(u32, HashMap<String, OwnedValue>), PortalError>
        where
            B: serde::Serialize + zbus::zvariant::DynamicType,
        {
            let expected = format!("{PATH}/request/{}/{token}", self.sender);
            let responses = self.responses(&expected)?;
            let reply = self
                .conn
                .call_method(Some(DEST), PATH, Some(IFACE), method, body)
                .map_err(|error| match error {
                    zbus::Error::MethodError(..) | zbus::Error::FDO(..) => failed(error),
                    other => PortalError::Unavailable(other.to_string()),
                })?;
            let handle: OwnedObjectPath = reply.body().deserialize().map_err(failed)?;
            // Portals older than 0.9 chose their own path: follow it.
            let responses = if handle.as_str() == expected {
                responses
            } else {
                self.responses(handle.as_str())?
            };
            wait_response(responses, timeout)
        }
    }

    impl PortalBus for ZbusPortal {
        fn create_session(&mut self) -> Result<String, PortalError> {
            let token = self.token();
            let session_token = self.token();
            let options: HashMap<&str, Value> = HashMap::from([
                ("handle_token", Value::from(token.as_str())),
                ("session_handle_token", Value::from(session_token.as_str())),
            ]);
            let (code, results) =
                self.request("CreateSession", &token, &(options,), CREATE_TIMEOUT)?;
            if code != 0 {
                return Err(failed(format!(
                    "the desktop refused to create a shortcut session (code {code})"
                )));
            }
            results
                .get("session_handle")
                .and_then(|value| text(value))
                .ok_or_else(|| failed("the desktop returned no session handle"))
        }

        fn bind(
            &mut self,
            session: &str,
            preferred_trigger: &str,
        ) -> Result<BindOutcome, PortalError> {
            let token = self.token();
            let session_path = ObjectPath::try_from(session).map_err(failed)?;
            let shortcut: HashMap<&str, Value> = HashMap::from([
                ("description", Value::from(ACTION_DESCRIPTION)),
                ("preferred_trigger", Value::from(preferred_trigger)),
            ]);
            let options: HashMap<&str, Value> =
                HashMap::from([("handle_token", Value::from(token.as_str()))]);
            let body = (session_path, vec![(ACTION_ID, shortcut)], "", options);
            let (code, results) = self.request("BindShortcuts", &token, &body, BIND_TIMEOUT)?;
            match code {
                0 => match results.get("shortcuts").map(|value| our_trigger(value)) {
                    // The desktop listed the shortcuts and ours is not among them.
                    Some(None) => Ok(BindOutcome::Rejected),
                    Some(Some(trigger)) => Ok(BindOutcome::Bound { trigger }),
                    None => Ok(BindOutcome::Bound { trigger: None }),
                },
                1 => Ok(BindOutcome::Rejected),
                other => Err(failed(format!(
                    "the desktop ended the shortcut request (code {other})"
                ))),
            }
        }

        fn close_session(&mut self, session: &str) {
            let _ = self
                .conn
                .call_method(Some(DEST), session, Some(SESSION_IFACE), "Close", &());
        }
    }

    enum Command {
        Apply(Option<String>, bool, mpsc::Sender<Result<(), String>>),
        /// `ShortcutsChanged` from the listener: session, new trigger.
        Changed(String, Option<String>),
        Close(mpsc::Sender<()>),
    }

    /// The running portal backend: a worker and a signal listener.
    pub struct PortalService {
        commands: mpsc::Sender<Command>,
        snapshot: SharedSnapshot,
    }

    fn publish(state: &PortalShortcut, snapshot: &SharedSnapshot) {
        *lock(snapshot) = PortalSnapshot {
            session: state.session().map(String::from),
            requested: state.requested().map(String::from),
            effective: state.effective().map(String::from),
            last_error: state.last_error().map(String::from),
        };
    }

    impl PortalService {
        /// Starts the worker and the listener. `on_toggle` runs on the
        /// listener thread for each press of our shortcut.
        pub fn start(
            connection: PortalConnection,
            on_toggle: impl Fn() + Send + 'static,
        ) -> Result<Self, String> {
            let conn = connection.conn;
            let snapshot = SharedSnapshot::default();

            let rule = MatchRule::builder()
                .msg_type(MessageType::Signal)
                .interface(IFACE)
                .and_then(|builder| builder.path(PATH))
                .map_err(|error| error.to_string())?
                .build();
            let signals = MessageIterator::for_match_rule(rule, &conn, Some(64))
                .map_err(|error| error.to_string())?;
            let (commands, receiver) = mpsc::channel();
            let listened = snapshot.clone();
            let changes = commands.clone();
            std::thread::Builder::new()
                .name("pulse-portal-signals".into())
                .spawn(move || listen(signals, &listened, &changes, on_toggle))
                .map_err(|error| error.to_string())?;

            let published = snapshot.clone();
            std::thread::Builder::new()
                .name("pulse-portal".into())
                .spawn(move || {
                    let mut bus = ZbusPortal::new(conn);
                    let mut state = PortalShortcut::default();
                    for command in receiver {
                        match command {
                            Command::Apply(wanted, explicit, reply) => {
                                let result = state.apply(&mut bus, wanted.as_deref(), explicit);
                                publish(&state, &published);
                                let _ = reply.send(result);
                            }
                            Command::Changed(session, trigger) => {
                                state.shortcuts_changed(&session, trigger);
                                publish(&state, &published);
                            }
                            Command::Close(reply) => {
                                state.close(&mut bus);
                                publish(&state, &published);
                                let _ = reply.send(());
                                break;
                            }
                        }
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(Self { commands, snapshot })
        }

        /// Binds `wanted` (or unbinds with `None`) and waits for the outcome —
        /// including, the first time, the user's answer to the desktop.
        pub fn apply(&self, wanted: Option<&str>, explicit: bool) -> Result<(), String> {
            let (reply, outcome) = mpsc::channel();
            self.commands
                .send(Command::Apply(wanted.map(String::from), explicit, reply))
                .map_err(|_| "the portal backend has stopped".to_string())?;
            outcome
                .recv()
                .unwrap_or_else(|_| Err("the portal backend has stopped".into()))
        }

        /// Closes the session (quitting PULSE); waits at most two seconds.
        pub fn close(&self) {
            let (reply, done) = mpsc::channel();
            if self.commands.send(Command::Close(reply)).is_ok() {
                let _ = done.recv_timeout(CLOSE_TIMEOUT);
            }
        }

        pub fn snapshot(&self) -> PortalSnapshot {
            lock(&self.snapshot).clone()
        }
    }

    fn listen(
        signals: MessageIterator,
        snapshot: &SharedSnapshot,
        changes: &mpsc::Sender<Command>,
        on_toggle: impl Fn(),
    ) {
        for message in signals {
            let Ok(message) = message else {
                continue;
            };
            let header = message.header();
            let Some(member) = header.member().map(|member| member.to_string()) else {
                continue;
            };
            match member.as_str() {
                "Activated" | "Deactivated" => {
                    let Ok((session, action, _timestamp, _options)) = message
                        .body()
                        .deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>(
                        )
                    else {
                        continue;
                    };
                    let active = lock(snapshot).session.clone();
                    let action =
                        signal_action(active.as_deref(), &member, session.as_str(), &action);
                    if action == SignalAction::Toggle {
                        on_toggle();
                    }
                }
                "ShortcutsChanged" => {
                    let Ok((session, shortcuts)) = message.body().deserialize::<(
                        OwnedObjectPath,
                        Vec<(String, HashMap<String, OwnedValue>)>,
                    )>() else {
                        continue;
                    };
                    let trigger = shortcuts
                        .iter()
                        .find(|(id, _)| id == ACTION_ID)
                        .and_then(|(_, properties)| properties.get("trigger_description"))
                        .and_then(|value| text(value));
                    if changes
                        .send(Command::Changed(session.to_string(), trigger))
                        .is_err()
                    {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::{PortalAvailability, PortalSnapshot};

    /// Never constructed: the portal exists only on Linux.
    pub enum PortalConnection {}

    pub fn probe() -> (PortalAvailability, Option<PortalConnection>) {
        (
            PortalAvailability::Unavailable {
                reason: "the XDG Desktop Portal exists only on Linux".into(),
            },
            None,
        )
    }

    pub enum PortalService {}

    impl PortalService {
        pub fn start(
            connection: PortalConnection,
            _on_toggle: impl Fn() + Send + 'static,
        ) -> Result<Self, String> {
            match connection {}
        }

        pub fn apply(&self, _wanted: Option<&str>, _explicit: bool) -> Result<(), String> {
            match *self {}
        }

        pub fn close(&self) {
            match *self {}
        }

        pub fn snapshot(&self) -> PortalSnapshot {
            match *self {}
        }
    }
}
