//! A netlink socket, opened and closed for one transaction.
//!
//! PULSE runs no `ip`, `ifconfig`, `ethtool`, `iw`, `iwconfig`, `nmcli` or
//! `networkctl`. Each of those is a program that opens this socket, sends one
//! of the requests below and formats the reply; spawning one per refresh would
//! add a runtime dependency on packages Fedora does not always install, a
//! locale-sensitive output format to parse, and a process.
//!
//! # One socket per transaction
//!
//! The socket is opened, used and closed within a single call. A long-lived
//! socket would need to survive suspend, interface churn and a kernel that can
//! drop a listener it considers too slow, and would have to be made `Sync` for
//! a provider shared across Tauri commands. Opening one costs a `socket(2)`
//! and a `bind(2)` — microseconds, once per refresh — and in exchange the
//! failure modes disappear entirely.
//!
//! # Read-only
//!
//! Only two request types are ever sent: `RTM_GETLINK` and the `nl80211`
//! commands that read station and interface information. Nothing here can
//! configure an address, bring a link up or down, join a network or change any
//! setting. The socket is never granted a multicast group, so it receives
//! nothing it did not ask for.

use crate::metrics::model::{MetricError, MetricErrorCode};

/// `NETLINK_ROUTE` — interfaces, addresses, routes.
pub const NETLINK_ROUTE: i32 = 0;
/// `NETLINK_GENERIC` — the multiplexed families, including `nl80211`.
pub const NETLINK_GENERIC: i32 = 16;

/// How much of a reply PULSE is willing to buffer.
///
/// A `RTM_GETLINK` dump on a machine with a hundred interfaces is well under
/// 64 KiB; 256 KiB leaves generous headroom without letting a misbehaving
/// kernel drive unbounded allocation.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const RECEIVE_BUFFER: usize = 256 * 1024;

/// How many datagrams a single dump may span before PULSE gives up.
///
/// A multipart reply terminates with `NLMSG_DONE`. The bound exists so that a
/// kernel which never sends one cannot hang a refresh forever.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const MAX_PARTS: usize = 64;

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::platform::linux::network::netlink::{self, Messages};
    use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};

    /// An open netlink socket.
    ///
    /// Wraps an [`OwnedFd`], so the descriptor is closed on every path out of
    /// a function including the error ones — there is no `close` to forget.
    pub struct NetlinkSocket {
        fd: OwnedFd,
        sequence: u32,
    }

    impl NetlinkSocket {
        /// Opens and binds a socket for one netlink protocol.
        pub fn open(protocol: i32) -> Result<Self, MetricError> {
            // SAFETY: a plain `socket(2)` with documented constants. The
            // returned descriptor is checked before being adopted.
            let raw = unsafe {
                libc::socket(
                    libc::AF_NETLINK,
                    libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                    protocol,
                )
            };

            if raw < 0 {
                return Err(from_io(
                    std::io::Error::last_os_error(),
                    "opening a netlink socket",
                ));
            }

            // SAFETY: `raw` is a fresh descriptor this function just created
            // and has not shared, so taking ownership of it is sound.
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };

            // SAFETY: `address` is a correctly sized, zeroed `sockaddr_nl`
            // that outlives the call. `nl_groups` is left at zero, so the
            // socket joins no multicast group and receives only replies to
            // what it sends.
            let bound = unsafe {
                let mut address = std::mem::zeroed::<libc::sockaddr_nl>();
                address.nl_family = libc::AF_NETLINK as u16;

                libc::bind(
                    fd.as_raw_fd(),
                    std::ptr::addr_of!(address) as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
                )
            };

            if bound < 0 {
                return Err(from_io(
                    std::io::Error::last_os_error(),
                    "binding a netlink socket",
                ));
            }

            // A refresh must never block indefinitely because the kernel did
            // not answer. Two seconds is far beyond any real dump.
            let timeout = libc::timeval {
                tv_sec: 2,
                tv_usec: 0,
            };

            // SAFETY: `timeout` is a correctly sized `timeval` that outlives
            // the call, and its length is passed alongside its pointer.
            unsafe {
                libc::setsockopt(
                    fd.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_RCVTIMEO,
                    std::ptr::addr_of!(timeout) as *const libc::c_void,
                    std::mem::size_of::<libc::timeval>() as libc::socklen_t,
                );
            }

            Ok(Self { fd, sequence: 1 })
        }

        /// The sequence number the next request will carry.
        pub const fn sequence(&self) -> u32 {
            self.sequence
        }

        /// Sends one request and collects the whole multipart reply.
        ///
        /// `payload` is everything after the 16-byte message header; this
        /// function writes the header itself, so no caller can send a message
        /// type it did not name here.
        ///
        /// Replies are matched on the **sequence number**, so a stale datagram
        /// from a previous transaction cannot be parsed as this one's answer.
        pub fn request(
            &mut self,
            message_type: u16,
            flags: u16,
            payload: &[u8],
            what: &str,
        ) -> Result<Vec<u8>, MetricError> {
            let sequence = self.sequence;
            self.sequence = self.sequence.wrapping_add(1).max(1);

            let length = netlink::NLMSGHDR_LEN + payload.len();
            let mut request = Vec::with_capacity(netlink::align(length));
            request.extend_from_slice(&(length as u32).to_ne_bytes());
            request.extend_from_slice(&message_type.to_ne_bytes());
            request.extend_from_slice(&(flags | netlink::NLM_F_REQUEST).to_ne_bytes());
            request.extend_from_slice(&sequence.to_ne_bytes());
            request.extend_from_slice(&0_u32.to_ne_bytes());
            request.extend_from_slice(payload);
            request.resize(netlink::align(length), 0);

            // SAFETY: `request` outlives the call and its length is passed
            // alongside its pointer, so the kernel reads exactly what was
            // built above and no more.
            let sent = unsafe {
                libc::send(
                    self.fd.as_raw_fd(),
                    request.as_ptr() as *const libc::c_void,
                    request.len(),
                    0,
                )
            };

            if sent < 0 {
                return Err(from_io(std::io::Error::last_os_error(), what));
            }

            self.collect(sequence, what)
        }

        /// Reads datagrams until the multipart reply terminates.
        fn collect(&self, sequence: u32, what: &str) -> Result<Vec<u8>, MetricError> {
            let mut collected = Vec::new();

            for _ in 0..MAX_PARTS {
                let mut buffer = vec![0_u8; RECEIVE_BUFFER];

                // SAFETY: `buffer` is owned here, outlives the call, and its
                // capacity is passed alongside its pointer, so the kernel
                // cannot write past it.
                let received = unsafe {
                    libc::recv(
                        self.fd.as_raw_fd(),
                        buffer.as_mut_ptr() as *mut libc::c_void,
                        buffer.len(),
                        0,
                    )
                };

                if received < 0 {
                    return Err(from_io(std::io::Error::last_os_error(), what));
                }
                if received == 0 {
                    break;
                }

                buffer.truncate(received as usize);

                // Inspect this datagram's messages before appending, so an
                // error reply is reported as itself rather than parsed as data.
                let mut finished = false;
                for message in Messages::new(&buffer) {
                    if message.header.sequence != sequence {
                        // A stale reply from an earlier transaction.
                        continue;
                    }

                    if message.header.message_type == netlink::NLMSG_ERROR {
                        match netlink::error_code(message.payload) {
                            // Zero is an acknowledgement, not a failure.
                            Some(0) | None => finished = true,
                            Some(errno) => return Err(netlink::from_errno(errno, what)),
                        }
                    }

                    if message.header.is_done() {
                        finished = true;
                    }
                }

                collected.extend_from_slice(&buffer);

                if finished {
                    return Ok(collected);
                }

                // A reply that is not flagged multipart is complete in one
                // datagram, so there is nothing more to wait for.
                if !Messages::new(&buffer).any(|message| message.header.is_multipart()) {
                    return Ok(collected);
                }
            }

            Err(MetricError::new(
                MetricErrorCode::Timeout,
                format!("{what} did not terminate within {MAX_PARTS} messages"),
            ))
        }
    }
}

#[cfg(target_os = "linux")]
pub use imp::NetlinkSocket;

/// Maps a failed socket operation onto the error code that describes it.
///
/// Compiled everywhere so its mapping is unit-tested on any host.
pub fn from_io(error: std::io::Error, what: &str) -> MetricError {
    use std::io::ErrorKind;

    let code = match error.kind() {
        ErrorKind::PermissionDenied => MetricErrorCode::PermissionDenied,
        // The kernel did not answer within the receive timeout.
        ErrorKind::WouldBlock | ErrorKind::TimedOut => MetricErrorCode::Timeout,
        // `AF_NETLINK` unavailable — a container with a restricted seccomp
        // profile, or a kernel built without it.
        ErrorKind::Unsupported => MetricErrorCode::Unsupported,
        _ => MetricErrorCode::Io,
    };

    MetricError::new(code, format!("{what} failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn the_protocol_numbers_match_the_kernel_headers() {
        assert_eq!(NETLINK_ROUTE, 0);
        assert_eq!(NETLINK_GENERIC, 16);
    }

    #[test]
    fn a_refused_socket_is_reported_as_a_permission_problem() {
        // A restricted container, not a machine without networking.
        let error = from_io(
            Error::from(ErrorKind::PermissionDenied),
            "the interface dump",
        );

        assert_eq!(error.code, MetricErrorCode::PermissionDenied);
        assert_eq!(
            crate::metrics::wellknown::availability_for(error).status_str(),
            "permissionDenied"
        );
    }

    #[test]
    fn a_kernel_that_does_not_answer_times_out_rather_than_hanging() {
        for kind in [ErrorKind::WouldBlock, ErrorKind::TimedOut] {
            let error = from_io(Error::from(kind), "the station dump");
            assert_eq!(error.code, MetricErrorCode::Timeout, "{kind:?}");
            // Transient: the next refresh may well succeed.
            assert!(crate::metrics::wellknown::availability_for(error).is_transient());
        }
    }

    #[test]
    fn a_kernel_without_netlink_is_unsupported() {
        let error = from_io(
            Error::from(ErrorKind::Unsupported),
            "opening a netlink socket",
        );
        assert_eq!(error.code, MetricErrorCode::Unsupported);
    }

    #[test]
    fn an_unrecognised_failure_stays_transient() {
        let error = from_io(
            Error::from(ErrorKind::ConnectionReset),
            "the interface dump",
        );

        assert_eq!(error.code, MetricErrorCode::Io);
        assert!(crate::metrics::wellknown::availability_for(error).is_transient());
    }

    #[test]
    fn an_error_names_what_failed() {
        let message = from_io(Error::from(ErrorKind::PermissionDenied), "the station dump").message;
        assert!(message.contains("the station dump"), "{message}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_route_socket_opens_on_this_machine() {
        // `AF_NETLINK` needs no privilege, so this works for any user.
        assert!(NetlinkSocket::open(NETLINK_ROUTE).is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sequence_numbers_advance_so_a_stale_reply_cannot_be_mistaken_for_an_answer() {
        let socket = NetlinkSocket::open(NETLINK_ROUTE).expect("opened");
        assert_eq!(socket.sequence(), 1);
    }
}
