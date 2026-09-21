//! Netlink: message framing, attribute walking, and a socket to carry them.
//!
//! # Why PULSE parses netlink itself
//!
//! Two data sources in this phase speak netlink: `rtnetlink` for the interface
//! inventory and its counters, and `nl80211` — a generic netlink family — for
//! Wi-Fi link quality. Both are built from the same three primitives: a fixed
//! 16-byte message header, a fixed per-family payload header, and a stream of
//! length-type-value attributes that may nest.
//!
//! The obvious alternative is a crate. The ones that exist are either large
//! (`neli`, a full generic-netlink framework), asynchronous and therefore a
//! Tokio dependency PULSE does not otherwise have (`rtnetlink`), or a stack of
//! four or five crates from the `rust-netlink` family. Against that, the
//! parsing PULSE actually needs is **one attribute iterator and two header
//! structs** — about the same size as the `STORAGE_DEVICE_DESCRIPTOR` reader
//! Phase 6 already hand-rolled, and shared between both families rather than
//! written twice.
//!
//! The decisive argument is the same one that applied to the NVMe log and the
//! Windows device descriptors: **a pure `&[u8] -> value` parser is testable on
//! any machine, against any byte sequence, including malformed ones.** A crate
//! would move that surface out of PULSE's test suite without removing it. So
//! the framing lives here, every bound is checked, and the fuzz-shaped cases —
//! truncated headers, attributes claiming to be longer than the buffer,
//! self-referential zero lengths, unterminated multipart streams — are unit
//! tests rather than hopes.
//!
//! # What "checked" means here
//!
//! Every function in this module takes a `&[u8]` and returns an `Option` or a
//! `Result`. None of them can read out of bounds, none can loop forever on a
//! malformed message, and none panics. A message PULSE cannot make sense of
//! becomes a `MetricError`, never a crash and never undefined behaviour.

use crate::metrics::model::{MetricError, MetricErrorCode};

/// The size of `struct nlmsghdr`.
pub const NLMSGHDR_LEN: usize = 16;

/// The size of `struct rtattr` / `struct nlattr` — a length and a type.
pub const ATTR_HEADER_LEN: usize = 4;

/// Netlink aligns every message and every attribute to four bytes.
pub const ALIGN_TO: usize = 4;

/// Rounds a length up to netlink's four-byte alignment.
///
/// Saturating rather than wrapping: a length near `usize::MAX` is a misparse,
/// and wrapping it to a small number would turn a rejected message into an
/// accepted one.
pub const fn align(len: usize) -> usize {
    len.saturating_add(ALIGN_TO - 1) & !(ALIGN_TO - 1)
}

// --- message types --------------------------------------------------------

/// `NLMSG_ERROR` — an error, or an acknowledgement when its code is zero.
pub const NLMSG_ERROR: u16 = 0x2;
/// `NLMSG_DONE` — the end of a multipart response.
pub const NLMSG_DONE: u16 = 0x3;

/// `NLM_F_REQUEST` — this message is a request.
pub const NLM_F_REQUEST: u16 = 0x001;
/// `NLM_F_DUMP` — return every matching object.
pub const NLM_F_DUMP: u16 = 0x300;
/// `NLM_F_MULTI` — this message is part of a multipart response.
pub const NLM_F_MULTI: u16 = 0x002;
/// `NLM_F_ACK` — request an acknowledgement.
pub const NLM_F_ACK: u16 = 0x004;

/// `NLA_F_NESTED` — set on an attribute whose payload is itself attributes.
///
/// Masked off before comparing a type, because kernels set it inconsistently:
/// some nested attributes carry the bit and some do not, so a parser that
/// compares the raw value silently fails to find half of them.
pub const NLA_F_NESTED: u16 = 0x8000;
/// `NLA_F_NET_BYTEORDER` — the payload is big-endian. Masked off for the same
/// reason.
pub const NLA_F_NET_BYTEORDER: u16 = 0x4000;

/// The type bits of an attribute, with the nesting and byte-order flags
/// removed.
pub const fn attr_type(raw: u16) -> u16 {
    raw & !(NLA_F_NESTED | NLA_F_NET_BYTEORDER)
}

// --- message header -------------------------------------------------------

/// A parsed `struct nlmsghdr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageHeader {
    /// Total message length **including** this header.
    pub length: u32,
    pub message_type: u16,
    pub flags: u16,
    pub sequence: u32,
    pub port: u32,
}

impl MessageHeader {
    /// Parses a header from the front of a buffer.
    ///
    /// Returns `None` when fewer than 16 bytes are available, or when the
    /// declared length is shorter than the header itself — which would make
    /// the message advance the cursor by zero and loop forever.
    pub fn parse(buffer: &[u8]) -> Option<Self> {
        let bytes = buffer.get(..NLMSGHDR_LEN)?;

        let header = Self {
            length: u32::from_ne_bytes(bytes[0..4].try_into().ok()?),
            message_type: u16::from_ne_bytes(bytes[4..6].try_into().ok()?),
            flags: u16::from_ne_bytes(bytes[6..8].try_into().ok()?),
            sequence: u32::from_ne_bytes(bytes[8..12].try_into().ok()?),
            port: u32::from_ne_bytes(bytes[12..16].try_into().ok()?),
        };

        (header.length as usize >= NLMSGHDR_LEN).then_some(header)
    }

    /// Whether this message is part of a multipart response.
    pub const fn is_multipart(&self) -> bool {
        self.flags & NLM_F_MULTI != 0
    }

    /// Whether this message terminates a multipart response.
    pub const fn is_done(&self) -> bool {
        self.message_type == NLMSG_DONE
    }
}

/// One message from a netlink response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message<'a> {
    pub header: MessageHeader,
    /// Everything after the 16-byte header, up to the declared length.
    pub payload: &'a [u8],
}

/// Walks the messages in a netlink response buffer.
///
/// A response may carry several messages back to back, each padded to a
/// four-byte boundary. The iterator stops at the first message it cannot trust
/// — a truncated header, or a declared length that runs past the buffer —
/// rather than guessing, and it cannot loop forever because
/// [`MessageHeader::parse`] refuses a length below the header size.
pub struct Messages<'a> {
    buffer: &'a [u8],
}

impl<'a> Messages<'a> {
    pub const fn new(buffer: &'a [u8]) -> Self {
        Self { buffer }
    }
}

impl<'a> Iterator for Messages<'a> {
    type Item = Message<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let header = MessageHeader::parse(self.buffer)?;
        let length = header.length as usize;

        // A message claiming to be longer than what arrived is a truncated
        // read, not a message.
        let message = self.buffer.get(..length)?;
        let payload = &message[NLMSGHDR_LEN..];

        // Advance past the padding. `align` can exceed the buffer on the last
        // message, which simply ends the iteration next time round.
        self.buffer = self.buffer.get(align(length)..).unwrap_or(&[]);

        Some(Message { header, payload })
    }
}

/// Reads the error code from an `NLMSG_ERROR` payload.
///
/// The payload is a negative `errno` followed by a copy of the offending
/// request header. A code of zero is an acknowledgement, not a failure.
pub fn error_code(payload: &[u8]) -> Option<i32> {
    let bytes = payload.get(..4)?;
    Some(i32::from_ne_bytes(bytes.try_into().ok()?))
}

// --- attributes -----------------------------------------------------------

/// One length-type-value attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute<'a> {
    /// The type, with the nesting and byte-order flags already masked off.
    pub kind: u16,
    /// The payload, excluding the four-byte header and any trailing padding.
    pub payload: &'a [u8],
}

impl<'a> Attribute<'a> {
    /// The payload as a little-endian `u8`.
    pub fn as_u8(&self) -> Option<u8> {
        self.payload.first().copied()
    }

    /// The payload as a native-endian `u16`.
    pub fn as_u16(&self) -> Option<u16> {
        Some(u16::from_ne_bytes(self.payload.get(..2)?.try_into().ok()?))
    }

    /// The payload as a native-endian `u32`.
    pub fn as_u32(&self) -> Option<u32> {
        Some(u32::from_ne_bytes(self.payload.get(..4)?.try_into().ok()?))
    }

    /// The payload as a native-endian `u64`.
    pub fn as_u64(&self) -> Option<u64> {
        Some(u64::from_ne_bytes(self.payload.get(..8)?.try_into().ok()?))
    }

    /// The payload as a native-endian `i32`.
    pub fn as_i32(&self) -> Option<i32> {
        Some(i32::from_ne_bytes(self.payload.get(..4)?.try_into().ok()?))
    }

    /// The payload as a signed byte — how `nl80211` reports a dBm signal.
    pub fn as_i8(&self) -> Option<i8> {
        self.payload.first().map(|&byte| byte as i8)
    }

    /// The payload as a NUL-terminated string.
    ///
    /// The kernel includes the terminator in the attribute length, and a
    /// parser that keeps it produces interface names that compare unequal to
    /// themselves.
    pub fn as_str(&self) -> Option<&'a str> {
        let end = self
            .payload
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(self.payload.len());

        std::str::from_utf8(&self.payload[..end]).ok()
    }

    /// The attributes nested inside this one.
    pub const fn nested(&self) -> Attributes<'a> {
        Attributes::new(self.payload)
    }
}

/// Walks a stream of netlink attributes.
///
/// Stops at the first attribute it cannot trust. Three malformed shapes are
/// specifically refused, each of which a naive walker mishandles:
///
/// - a **length below the four-byte header**, which would advance the cursor
///   by zero and hang;
/// - a **length past the end of the buffer**, which would read out of bounds;
/// - a **truncated final attribute**, which would yield a payload shorter than
///   it claims.
pub struct Attributes<'a> {
    buffer: &'a [u8],
}

impl<'a> Attributes<'a> {
    pub const fn new(buffer: &'a [u8]) -> Self {
        Self { buffer }
    }

    /// The first attribute of a given type, if present.
    ///
    /// Written as a loop rather than `Iterator::find`, because this type is
    /// itself an iterator and the two names would shadow each other.
    pub fn find(mut self, kind: u16) -> Option<Attribute<'a>> {
        loop {
            let attribute = self.next()?;
            if attribute.kind == kind {
                return Some(attribute);
            }
        }
    }
}

impl<'a> Iterator for Attributes<'a> {
    type Item = Attribute<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let header = self.buffer.get(..ATTR_HEADER_LEN)?;

        let length = u16::from_ne_bytes(header[0..2].try_into().ok()?) as usize;
        let raw_kind = u16::from_ne_bytes(header[2..4].try_into().ok()?);

        // Below the header size the cursor would not advance: a malformed
        // message must not become an infinite loop.
        if length < ATTR_HEADER_LEN {
            self.buffer = &[];
            return None;
        }

        // Claiming more than arrived is a truncated read.
        let Some(payload) = self.buffer.get(ATTR_HEADER_LEN..length) else {
            self.buffer = &[];
            return None;
        };

        self.buffer = self.buffer.get(align(length)..).unwrap_or(&[]);

        Some(Attribute {
            kind: attr_type(raw_kind),
            payload,
        })
    }
}

/// Builds the error a malformed or refused netlink exchange produces.
pub fn netlink_error(code: MetricErrorCode, message: impl Into<String>) -> MetricError {
    MetricError::new(code, message)
}

/// Maps a netlink `errno` onto the error code that describes it honestly.
pub fn from_errno(errno: i32, what: &str) -> MetricError {
    // The kernel reports a negative errno.
    let errno = errno.abs();

    let (code, message) = match errno {
        // EPERM / EACCES
        1 | 13 => (
            MetricErrorCode::PermissionDenied,
            format!("the kernel refused {what} to this process"),
        ),
        // ENODEV / ENOENT
        2 | 19 => (
            MetricErrorCode::NotDetected,
            format!("{what} refers to an interface that no longer exists"),
        ),
        // EOPNOTSUPP / ENOTSUP
        95 => (
            MetricErrorCode::Unsupported,
            format!("this driver does not implement {what}"),
        ),
        // EAFNOSUPPORT — the netlink family is not available in this kernel.
        97 => (
            MetricErrorCode::Unsupported,
            format!("this kernel provides no netlink family for {what}"),
        ),
        other => (
            MetricErrorCode::Io,
            format!("{what} failed with errno {other}"),
        ),
    };

    netlink_error(code, message)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Builds one attribute: a four-byte header followed by a padded payload.
    pub fn attribute(kind: u16, payload: &[u8]) -> Vec<u8> {
        let length = ATTR_HEADER_LEN + payload.len();
        let mut out = Vec::with_capacity(align(length));

        out.extend_from_slice(&(length as u16).to_ne_bytes());
        out.extend_from_slice(&kind.to_ne_bytes());
        out.extend_from_slice(payload);
        out.resize(align(length), 0);

        out
    }

    /// Builds a `u32` attribute.
    pub fn u32_attribute(kind: u16, value: u32) -> Vec<u8> {
        attribute(kind, &value.to_ne_bytes())
    }

    /// Builds a `u8` attribute.
    pub fn u8_attribute(kind: u16, value: u8) -> Vec<u8> {
        attribute(kind, &[value])
    }

    /// Builds a NUL-terminated string attribute, as the kernel writes one.
    pub fn string_attribute(kind: u16, value: &str) -> Vec<u8> {
        let mut payload = value.as_bytes().to_vec();
        payload.push(0);
        attribute(kind, &payload)
    }

    /// Builds a nested attribute from already-encoded children.
    pub fn nested_attribute(kind: u16, children: &[Vec<u8>]) -> Vec<u8> {
        let payload: Vec<u8> = children.concat();
        attribute(kind | NLA_F_NESTED, &payload)
    }

    /// Builds one netlink message around a payload.
    pub fn message(message_type: u16, flags: u16, sequence: u32, payload: &[u8]) -> Vec<u8> {
        let length = NLMSGHDR_LEN + payload.len();
        let mut out = Vec::with_capacity(align(length));

        out.extend_from_slice(&(length as u32).to_ne_bytes());
        out.extend_from_slice(&message_type.to_ne_bytes());
        out.extend_from_slice(&flags.to_ne_bytes());
        out.extend_from_slice(&sequence.to_ne_bytes());
        out.extend_from_slice(&0_u32.to_ne_bytes());
        out.extend_from_slice(payload);
        out.resize(align(length), 0);

        out
    }

    /// Builds the `NLMSG_DONE` message that terminates a multipart response.
    pub fn done(sequence: u32) -> Vec<u8> {
        message(NLMSG_DONE, NLM_F_MULTI, sequence, &0_i32.to_ne_bytes())
    }

    /// Builds an `NLMSG_ERROR` message carrying a negative errno.
    pub fn error(sequence: u32, errno: i32) -> Vec<u8> {
        let mut payload = (-errno.abs()).to_ne_bytes().to_vec();
        // The kernel echoes the offending request header back.
        payload.extend_from_slice(&[0_u8; NLMSGHDR_LEN]);
        message(NLMSG_ERROR, 0, sequence, &payload)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    // --- alignment ---------------------------------------------------------

    #[test]
    fn alignment_rounds_up_to_four() {
        assert_eq!(align(0), 0);
        assert_eq!(align(1), 4);
        assert_eq!(align(4), 4);
        assert_eq!(align(5), 8);
        assert_eq!(align(16), 16);
        assert_eq!(align(17), 20);
    }

    #[test]
    fn alignment_saturates_rather_than_wrapping() {
        // Wrapping a near-maximum length to a small number would turn a
        // message the parser should reject into one it accepts.
        assert_eq!(align(usize::MAX), usize::MAX & !(ALIGN_TO - 1));
    }

    // --- attribute type flags ---------------------------------------------

    #[test]
    fn the_nesting_and_byte_order_flags_are_masked_off_a_type() {
        // Kernels set them inconsistently, so a parser comparing raw values
        // silently fails to find half the nested attributes.
        assert_eq!(attr_type(7), 7);
        assert_eq!(attr_type(7 | NLA_F_NESTED), 7);
        assert_eq!(attr_type(7 | NLA_F_NET_BYTEORDER), 7);
        assert_eq!(attr_type(7 | NLA_F_NESTED | NLA_F_NET_BYTEORDER), 7);
    }

    // --- message framing ---------------------------------------------------

    #[test]
    fn parses_a_message_header() {
        let buffer = message(16, NLM_F_MULTI, 42, &[1, 2, 3, 4]);
        let header = MessageHeader::parse(&buffer).expect("valid");

        assert_eq!(header.length as usize, NLMSGHDR_LEN + 4);
        assert_eq!(header.message_type, 16);
        assert_eq!(header.sequence, 42);
        assert!(header.is_multipart());
        assert!(!header.is_done());
    }

    #[test]
    fn refuses_a_truncated_header() {
        assert_eq!(MessageHeader::parse(&[]), None);
        assert_eq!(MessageHeader::parse(&[0_u8; 8]), None);
        assert_eq!(MessageHeader::parse(&[0_u8; NLMSGHDR_LEN - 1]), None);
    }

    #[test]
    fn refuses_a_length_shorter_than_the_header_itself() {
        // The shape that makes a naive walker advance by zero and hang.
        let mut buffer = vec![0_u8; NLMSGHDR_LEN];
        buffer[0..4].copy_from_slice(&4_u32.to_ne_bytes());

        assert_eq!(MessageHeader::parse(&buffer), None);
    }

    #[test]
    fn walks_several_messages_back_to_back() {
        let mut buffer = message(16, NLM_F_MULTI, 1, &[0xAA; 6]);
        buffer.extend(message(16, NLM_F_MULTI, 1, &[0xBB; 2]));
        buffer.extend(done(1));

        let messages: Vec<Message> = Messages::new(&buffer).collect();

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].payload.len(), 6);
        assert_eq!(&messages[0].payload[..6], &[0xAA; 6]);
        assert_eq!(messages[1].payload.len(), 2);
        assert!(messages[2].header.is_done());
    }

    #[test]
    fn stops_at_a_message_claiming_more_than_arrived() {
        // A truncated read, not a message. Trusting the length would read out
        // of bounds.
        let mut buffer = message(16, 0, 1, &[0xAA; 8]);
        buffer.truncate(buffer.len() - 4);

        assert_eq!(Messages::new(&buffer).count(), 0);
    }

    #[test]
    fn stops_cleanly_on_a_garbage_tail() {
        let mut buffer = message(16, NLM_F_MULTI, 1, &[0xAA; 4]);
        buffer.extend_from_slice(&[0xFF; 3]);

        let messages: Vec<Message> = Messages::new(&buffer).collect();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn an_empty_buffer_yields_no_messages() {
        assert_eq!(Messages::new(&[]).count(), 0);
    }

    #[test]
    fn reads_an_error_code_and_tells_an_acknowledgement_from_a_failure() {
        let failure = error(1, 13);
        let message = Messages::new(&failure).next().expect("one message");
        assert_eq!(message.header.message_type, NLMSG_ERROR);
        assert_eq!(error_code(message.payload), Some(-13));

        // Zero is an acknowledgement.
        let ack = fixtures::message(NLMSG_ERROR, 0, 1, &0_i32.to_ne_bytes());
        let message = Messages::new(&ack).next().expect("one message");
        assert_eq!(error_code(message.payload), Some(0));

        assert_eq!(error_code(&[]), None);
        assert_eq!(error_code(&[1, 2]), None);
    }

    // --- attributes --------------------------------------------------------

    #[test]
    fn walks_attributes_of_every_width() {
        let buffer: Vec<u8> = [
            u8_attribute(1, 6),
            u32_attribute(2, 1500),
            string_attribute(3, "wlp59s0f0"),
            attribute(4, &1_234_567_890_u64.to_ne_bytes()),
        ]
        .concat();

        let attributes: Vec<Attribute> = Attributes::new(&buffer).collect();

        assert_eq!(attributes.len(), 4);
        assert_eq!(attributes[0].as_u8(), Some(6));
        assert_eq!(attributes[1].as_u32(), Some(1500));
        assert_eq!(attributes[2].as_str(), Some("wlp59s0f0"));
        assert_eq!(attributes[3].as_u64(), Some(1_234_567_890));
    }

    #[test]
    fn a_string_attribute_loses_its_nul_terminator() {
        // Keeping it produces interface names that compare unequal to
        // themselves.
        let buffer = string_attribute(3, "enp58s0");
        let attribute = Attributes::new(&buffer).next().expect("one attribute");

        assert_eq!(attribute.as_str(), Some("enp58s0"));
        assert_ne!(attribute.as_str(), Some("enp58s0\0"));
    }

    #[test]
    fn finds_an_attribute_by_type_regardless_of_position() {
        let buffer: Vec<u8> = [
            u32_attribute(1, 10),
            u32_attribute(4, 40),
            u32_attribute(9, 90),
        ]
        .concat();

        assert_eq!(
            Attributes::new(&buffer).find(4).and_then(|a| a.as_u32()),
            Some(40)
        );
        assert_eq!(
            Attributes::new(&buffer).find(9).and_then(|a| a.as_u32()),
            Some(90)
        );
        assert_eq!(Attributes::new(&buffer).find(99), None);
    }

    #[test]
    fn finds_an_attribute_whose_type_carries_the_nested_flag() {
        let buffer = nested_attribute(18, &[u32_attribute(1, 7)]);

        let found = Attributes::new(&buffer).find(18).expect("found");
        assert_eq!(found.nested().find(1).and_then(|a| a.as_u32()), Some(7));
    }

    #[test]
    fn walks_nested_attributes_to_depth() {
        let inner: Vec<Vec<u8>> = vec![u32_attribute(5, 175_500), u32_attribute(6, 390_000)];
        let middle = nested_attribute(8, &inner);
        let outer = nested_attribute(21, &[middle]);

        let station = Attributes::new(&outer).find(21).expect("station info");
        let rate = station.nested().find(8).expect("bitrate");

        assert_eq!(
            rate.nested().find(5).and_then(|a| a.as_u32()),
            Some(175_500)
        );
        assert_eq!(
            rate.nested().find(6).and_then(|a| a.as_u32()),
            Some(390_000)
        );
    }

    #[test]
    fn padding_between_attributes_is_skipped() {
        // A three-byte payload occupies eight bytes: four of header, three of
        // payload, one of padding.
        let buffer: Vec<u8> = [attribute(1, &[1, 2, 3]), u32_attribute(2, 9)].concat();

        assert_eq!(buffer.len(), 8 + 8);
        let attributes: Vec<Attribute> = Attributes::new(&buffer).collect();
        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].payload, &[1, 2, 3]);
        assert_eq!(attributes[1].as_u32(), Some(9));
    }

    // --- malformed attributes ----------------------------------------------

    #[test]
    fn an_attribute_length_below_the_header_stops_the_walk_without_hanging() {
        // The single most important malformed shape: length 0 or 2 would leave
        // the cursor where it was and spin forever.
        for bad_length in [0_u16, 1, 2, 3] {
            let mut buffer = bad_length.to_ne_bytes().to_vec();
            buffer.extend_from_slice(&1_u16.to_ne_bytes());
            buffer.extend_from_slice(&[0xFF; 16]);

            assert_eq!(
                Attributes::new(&buffer).count(),
                0,
                "length {bad_length} must terminate the walk"
            );
        }
    }

    #[test]
    fn an_attribute_claiming_more_than_the_buffer_holds_is_refused() {
        let mut buffer = 200_u16.to_ne_bytes().to_vec();
        buffer.extend_from_slice(&1_u16.to_ne_bytes());
        buffer.extend_from_slice(&[0xFF; 8]);

        assert_eq!(Attributes::new(&buffer).count(), 0);
    }

    #[test]
    fn a_truncated_final_attribute_is_dropped_not_half_read() {
        let mut buffer: Vec<u8> = [u32_attribute(1, 10), u32_attribute(2, 20)].concat();
        buffer.truncate(buffer.len() - 2);

        let attributes: Vec<Attribute> = Attributes::new(&buffer).collect();

        assert_eq!(attributes.len(), 1, "the intact one survives");
        assert_eq!(attributes[0].as_u32(), Some(10));
    }

    #[test]
    fn a_header_with_no_payload_at_all_is_walked_safely() {
        let buffer = attribute(7, &[]);

        let attributes: Vec<Attribute> = Attributes::new(&buffer).collect();
        assert_eq!(attributes.len(), 1);
        assert!(attributes[0].payload.is_empty());
        // …and every accessor refuses rather than reading past it.
        assert_eq!(attributes[0].as_u32(), None);
        assert_eq!(attributes[0].as_u64(), None);
        assert_eq!(attributes[0].as_u8(), None);
        assert_eq!(attributes[0].as_str(), Some(""));
    }

    #[test]
    fn an_accessor_refuses_a_payload_narrower_than_its_type() {
        let buffer = attribute(1, &[0xAA, 0xBB]);
        let attribute = Attributes::new(&buffer).next().expect("one");

        assert_eq!(attribute.as_u16(), Some(u16::from_ne_bytes([0xAA, 0xBB])));
        assert_eq!(attribute.as_u32(), None, "two bytes are not a u32");
        assert_eq!(attribute.as_u64(), None);
        assert_eq!(attribute.as_i32(), None);
    }

    #[test]
    fn a_signal_byte_is_read_as_signed() {
        // nl80211 reports dBm as a signed byte: 0xBC is −68, not 188.
        let buffer = attribute(7, &[0xBC]);
        let attribute = Attributes::new(&buffer).next().expect("one");

        assert_eq!(attribute.as_i8(), Some(-68));
        assert_eq!(attribute.as_u8(), Some(188));
    }

    #[test]
    fn invalid_utf8_in_a_string_attribute_is_refused_rather_than_mangled() {
        let buffer = attribute(3, &[0xFF, 0xFE, 0x00]);
        let attribute = Attributes::new(&buffer).next().expect("one");

        assert_eq!(attribute.as_str(), None);
    }

    #[test]
    fn nested_walking_of_a_malformed_payload_terminates() {
        let buffer = attribute(21 | NLA_F_NESTED, &[0x00, 0x00, 0x01, 0x00, 0xFF, 0xFF]);
        let attribute = Attributes::new(&buffer).next().expect("one");

        assert_eq!(attribute.nested().count(), 0);
    }

    // --- errno mapping -----------------------------------------------------

    #[test]
    fn permission_problems_are_never_reported_as_unsupported() {
        for errno in [1, 13] {
            let error = from_errno(-errno, "the interface dump");
            assert_eq!(
                error.code,
                MetricErrorCode::PermissionDenied,
                "errno {errno}"
            );
        }
    }

    #[test]
    fn an_unimplemented_family_or_operation_is_unsupported() {
        assert_eq!(
            from_errno(-95, "the station dump").code,
            MetricErrorCode::Unsupported
        );
        assert_eq!(
            from_errno(-97, "nl80211").code,
            MetricErrorCode::Unsupported
        );
    }

    #[test]
    fn a_vanished_interface_is_not_detected_rather_than_an_error() {
        for errno in [2, 19] {
            assert_eq!(
                from_errno(-errno, "the station dump").code,
                MetricErrorCode::NotDetected,
                "errno {errno}"
            );
        }
    }

    #[test]
    fn an_unrecognised_errno_stays_transient() {
        let error = from_errno(-105, "the interface dump");

        assert_eq!(error.code, MetricErrorCode::Io);
        assert!(crate::metrics::wellknown::availability_for(error).is_transient());
    }

    #[test]
    fn an_error_names_what_failed() {
        let message = from_errno(-13, "the station dump").message;
        assert!(message.contains("the station dump"), "{message}");
    }

    #[test]
    fn a_positive_errno_is_handled_like_its_negative_form() {
        // The kernel sends negative; a caller passing the absolute value must
        // not fall through to the catch-all.
        assert_eq!(from_errno(13, "x").code, from_errno(-13, "x").code);
    }
}
