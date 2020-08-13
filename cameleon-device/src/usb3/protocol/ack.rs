use std::io::Cursor;
use std::time;

use byteorder::{ReadBytesExt, LE};

use crate::usb3::{Error, Result};

use super::parse_util;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AckPacket<'a> {
    ccd: AckCcd,
    pub scd: Option<AckScd<'a>>,
}

impl<'a> AckPacket<'a> {
    const PREFIX_MAGIC: u32 = 0x43563355;

    pub fn parse(buf: &'a (impl AsRef<[u8]> + ?Sized)) -> Result<Self> {
        let mut cursor = Cursor::new(buf.as_ref());

        Self::parse_prefix(&mut cursor)?;

        let ccd = AckCcd::parse(&mut cursor)?;

        let scd = if ccd.status.is_success() {
            Some(AckScd::parse(&mut cursor, &ccd)?)
        } else {
            None
        };

        Ok(Self { ccd, scd })
    }

    pub fn status(&self) -> &Status {
        &self.ccd.status
    }

    pub fn request_id(&self) -> u16 {
        self.ccd.request_id
    }

    pub fn custom_command_id(&self) -> Option<u16> {
        match self.ccd.scd_kind {
            ScdKind::Custom(id) => Some(id),
            _ => None,
        }
    }

    fn parse_prefix(cursor: &mut Cursor<&[u8]>) -> Result<()> {
        let magic = cursor.read_u32::<LE>()?;
        if magic == Self::PREFIX_MAGIC {
            Ok(())
        } else {
            Err(Error::InvalidPacket("invalid prefix magic".into()))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    code: u16,
    kind: StatusKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusKind {
    GenCp(GenCpStatus),
    UsbSpecific(UsbSpecificStatus),
    DeviceSpecific,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenCpStatus {
    /// Success.
    Success,

    /// Command not implemented in the device.
    NotImplemented,

    /// Command parameter of CCD or SCD is invalid.
    InvalidParameter,

    /// Attempt to access an address that doesn't exist.
    InvalidAddress,

    /// Attempt to write to a read only address.
    WriteProtect,

    /// Attempt to access an address with bad alignment.
    BadAlignment,

    /// Attempt to read unreadable address or write to unwritable address.
    AccessDenied,

    /// The command receiver is busy.
    Busy,

    /// Timeout waiting for an acknowledge.
    Timeout,

    /// Header is inconsistent with data.
    InvalidHeader,

    /// The receiver configuration does not allow the execution of the sent command.
    WrongConfig,

    /// Generic error.
    GenericError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsbSpecificStatus {
    /// Resend command is not supported by USB device.
    ResendNotSupported,

    /// Stream endpoint is halted when stream flag is set.
    StreamEndpointHalted,

    /// Command that attempts to set payload size is invalid because of bad alignment.
    PayloadSizeNotAligned,

    /// Event endpoint is halted when event enable flag is set.
    EventEndpointHalted,

    /// Command that attempts to enable stream is failed because streaming interface is invalid
    /// state.
    InvalidSiState,
}

impl Status {
    pub fn is_success(&self) -> bool {
        match self.kind {
            StatusKind::GenCp(GenCpStatus::Success) => true,
            _ => false,
        }
    }

    pub fn is_fatal(&self) -> bool {
        self.code >> 15 == 1
    }

    pub fn code(&self) -> u16 {
        self.code
    }

    fn parse(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        let code = cursor.read_u16::<LE>()?;

        let namespace = (code >> 13) & 0x11;
        match namespace {
            0b00 => Self::parse_gencp_status(code),
            0b01 => Self::parse_usb_status(code),
            0b10 => Ok(Self {
                code,
                kind: StatusKind::DeviceSpecific,
            }),
            _ => Err(Error::InvalidPacket(
                "invalid ack status code, namespace is set to 0b11".into(),
            )),
        }
    }

    fn parse_gencp_status(code: u16) -> Result<Self> {
        use GenCpStatus::*;

        debug_assert!((code >> 13).trailing_zeros() >= 2);

        let status = match code {
            0x0000 => Success,
            0x8001 => NotImplemented,
            0x8002 => InvalidParameter,
            0x8003 => InvalidAddress,
            0x8004 => WriteProtect,
            0x8005 => BadAlignment,
            0x8006 => AccessDenied,
            0x8007 => Busy,
            0x800B => Timeout,
            0x800E => InvalidHeader,
            0x800F => WrongConfig,
            0x8FFF => GenericError,
            _ => {
                return Err(Error::InvalidPacket(
                    format! {"invalid gencp status code {:#X}", code}.into(),
                ))
            }
        };

        Ok(Self {
            code,
            kind: StatusKind::GenCp(status),
        })
    }

    fn parse_usb_status(code: u16) -> Result<Self> {
        use UsbSpecificStatus::*;

        debug_assert!(code >> 13 & 0b11 == 0b01);

        let status = match code {
            0xA001 => ResendNotSupported,
            0xA002 => StreamEndpointHalted,
            0xA003 => PayloadSizeNotAligned,
            0xA004 => InvalidSiState,
            0xA005 => EventEndpointHalted,
            _ => {
                return Err(Error::InvalidPacket(
                    format! {"invalid usb status code {:#X}", code}.into(),
                ))
            }
        };

        Ok(Self {
            code,
            kind: StatusKind::UsbSpecific(status),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AckScd<'a> {
    ReadMem { data: &'a [u8] },
    WriteMem { length: u16 },
    ReadMemStacked { data: &'a [u8] },
    WriteMemStacked { lengths: Vec<u16> },
    Pending { timeout: time::Duration },
    Custom { data: &'a [u8] },
}

impl<'a> AckScd<'a> {
    fn parse(cursor: &mut Cursor<&'a [u8]>, ccd: &AckCcd) -> Result<Self> {
        match ccd.scd_kind {
            ScdKind::ReadMem => Self::parse_read_mem(cursor, ccd.scd_len),
            ScdKind::WriteMem => Self::parse_write_mem(cursor),
            ScdKind::ReadMemStacked => Self::parse_read_mem_stacked(cursor, ccd.scd_len),
            ScdKind::WriteMemStacked => Self::parse_write_mem_stacked(cursor, ccd.scd_len),
            ScdKind::Pending => Self::parse_pending(cursor),
            ScdKind::Custom(..) => Self::parse_custom(cursor, ccd.scd_len),
        }
    }

    fn parse_read_mem(cursor: &mut Cursor<&'a [u8]>, len: u16) -> Result<Self> {
        let data = parse_util::read_bytes(cursor, len)?;
        Ok(AckScd::ReadMem { data })
    }

    fn parse_write_mem(cursor: &mut Cursor<&'a [u8]>) -> Result<Self> {
        let reserved = cursor.read_u16::<LE>()?;
        if reserved != 0 {
            return Err(Error::InvalidPacket(
                "the first two bytes of WriteMemAck scd must be set to zero".into(),
            ));
        }

        let length = cursor.read_u16::<LE>()?;
        Ok(Self::WriteMem { length })
    }

    fn parse_read_mem_stacked(cursor: &mut Cursor<&'a [u8]>, len: u16) -> Result<Self> {
        let data = parse_util::read_bytes(cursor, len)?;
        Ok(AckScd::ReadMemStacked { data })
    }

    fn parse_write_mem_stacked(cursor: &mut Cursor<&'a [u8]>, mut len: u16) -> Result<Self> {
        let mut lengths = Vec::with_capacity(len as usize % 4);

        while len > 0 {
            let reserved = cursor.read_u16::<LE>()?;
            if reserved != 0 {
                return Err(Error::InvalidPacket(
                    "the first two bytes of each WriteMemStackedAck scd entry must be set to zero"
                        .into(),
                ));
            }
            let length = cursor.read_u16::<LE>()?;
            lengths.push(length);
            len = len.saturating_sub(4);
        }

        Ok(Self::WriteMemStacked { lengths })
    }

    fn parse_pending(cursor: &mut Cursor<&'a [u8]>) -> Result<Self> {
        let reserved = cursor.read_u16::<LE>()?;
        if reserved != 0 {
            return Err(Error::InvalidPacket(
                "the first two bytes of PendingAck scd must be set to zero".into(),
            ));
        }

        let timeout_ms = cursor.read_u16::<LE>()?;
        let timeout = time::Duration::from_millis(timeout_ms.into());
        Ok(Self::Pending { timeout })
    }

    fn parse_custom(cursor: &mut Cursor<&'a [u8]>, len: u16) -> Result<Self> {
        let data = parse_util::read_bytes(cursor, len)?;
        Ok(AckScd::Custom { data })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AckCcd {
    status: Status,
    scd_kind: ScdKind,
    request_id: u16,
    scd_len: u16,
}

impl AckCcd {
    fn parse(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        let status = Status::parse(cursor)?;
        let scd_kind = ScdKind::parse(cursor)?;
        let scd_len = cursor.read_u16::<LE>()?;
        let request_id = cursor.read_u16::<LE>()?;

        Ok(Self {
            status,
            scd_kind,
            scd_len,
            request_id,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ScdKind {
    ReadMem,
    WriteMem,
    ReadMemStacked,
    WriteMemStacked,
    Pending,
    Custom(u16),
}

impl ScdKind {
    fn parse(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        let id = cursor.read_u16::<LE>()?;
        match id {
            0x0801 => Ok(ScdKind::ReadMem),
            0x0803 => Ok(ScdKind::WriteMem),
            0x0805 => Ok(ScdKind::Pending),
            0x0807 => Ok(ScdKind::ReadMemStacked),
            0x0809 => Ok(ScdKind::WriteMemStacked),
            _ if (id >> 15 == 1 && id & 1 == 1) => Ok(ScdKind::Custom(id)),
            _ => Err(Error::InvalidPacket(
                format!("unknown ack command id {:#X}", id).into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::WriteBytesExt;

    fn serialize_header(
        status_code: u16,
        command_id: u16,
        scd_len: u16,
        request_id: u16,
    ) -> Vec<u8> {
        let mut ccd = vec![];
        ccd.write_u32::<LE>(0x43563355).unwrap();
        ccd.write_u16::<LE>(status_code).unwrap();
        ccd.write_u16::<LE>(command_id).unwrap();
        ccd.write_u16::<LE>(scd_len).unwrap();
        ccd.write_u16::<LE>(request_id).unwrap();
        ccd
    }

    #[test]
    fn test_read_mem_ack() {
        let scd = &[0x01, 0x02, 0x03, 0x04];
        let mut raw_packet = serialize_header(0x0000, 0x0801, scd.len() as u16, 1);
        raw_packet.extend(scd);

        let ack = AckPacket::parse(&raw_packet).unwrap();
        assert!(ack.status().is_success());
        assert!(!ack.status().is_fatal());
        assert_eq!(ack.request_id(), 1);
        assert!(ack.custom_command_id().is_none());

        match ack.scd {
            Some(AckScd::ReadMem { data }) => {
                assert_eq!(data, scd);
            }
            other => panic!("unexpected scd, {:?}", other),
        }
    }

    #[test]
    fn test_write_mem_ack() {
        let scd = &[0x00, 0x00, 0x0a, 0x00]; // Written length is 10.
        let mut raw_packet = serialize_header(0x0000, 0x0803, scd.len() as u16, 1);
        raw_packet.extend(scd);

        let ack = AckPacket::parse(&raw_packet).unwrap();
        assert_eq!(ack.status().code(), 0x0000);
        assert!(ack.status().is_success());
        assert!(!ack.status().is_fatal());
        assert_eq!(ack.request_id(), 1);
        assert!(ack.custom_command_id().is_none());

        match ack.scd {
            Some(AckScd::WriteMem { length }) => {
                assert_eq!(length, 0x0a);
            }
            other => panic!("unexpected scd, {:?}", other),
        }
    }

    #[test]
    fn test_read_mem_stacked_ack() {
        let scd = &[0x01, 0x02, 0x03, 0x04];
        let mut raw_packet = serialize_header(0x0000, 0x0807, scd.len() as u16, 1);
        raw_packet.extend(scd);

        let ack = AckPacket::parse(&raw_packet).unwrap();
        assert_eq!(ack.status().code(), 0x0000);
        assert!(ack.status().is_success());
        assert!(!ack.status().is_fatal());
        assert_eq!(ack.request_id(), 1);
        assert!(ack.custom_command_id().is_none());

        match ack.scd {
            Some(AckScd::ReadMemStacked { data }) => {
                assert_eq!(data, scd);
            }
            other => panic!("unexpected scd, {:?}", other),
        }
    }

    #[test]
    fn test_write_mem_stacked_ack() {
        let mut scd = vec![0x00, 0x00, 0x03, 0x00]; // Written length 0: 3 bytes written.
        scd.extend(&[0x00, 0x00, 0x0a, 0x00]); // Written length 1: 10 bytes written.
        let mut raw_packet = serialize_header(0x0000, 0x0809, scd.len() as u16, 1);
        raw_packet.extend(&scd);

        let ack = AckPacket::parse(&raw_packet).unwrap();
        assert_eq!(ack.status().code(), 0x0000);
        assert!(ack.status().is_success());
        assert!(!ack.status().is_fatal());
        assert_eq!(ack.request_id(), 1);
        assert!(ack.custom_command_id().is_none());

        match ack.scd {
            Some(AckScd::WriteMemStacked { lengths }) => {
                assert_eq!(lengths, &[3, 10]);
            }
            other => panic!("unexpected scd, {:?}", other),
        }
    }

    #[test]
    fn test_pending_ack() {
        use std::time::Duration;

        let scd = &[0x00, 0x00, 0xbc, 0x02]; // Timeout is 700 ms.
        let mut raw_packet = serialize_header(0x0000, 0x0805, scd.len() as u16, 1);
        raw_packet.extend(scd);

        let ack = AckPacket::parse(&raw_packet).unwrap();
        assert_eq!(ack.status().code(), 0x0000);
        assert!(ack.status().is_success());
        assert!(!ack.status().is_fatal());
        assert_eq!(ack.request_id(), 1);
        assert!(ack.custom_command_id().is_none());

        match ack.scd {
            Some(AckScd::Pending { timeout }) => {
                assert_eq!(timeout, Duration::from_millis(700));
            }
            other => panic!("unexpected scd, {:?}", other),
        }
    }

    #[test]
    fn test_gencp_error_status() {
        let mut code_buf = vec![0; 2];

        code_buf.as_mut_slice().write_u16::<LE>(0x800F).unwrap();
        let mut code = Cursor::new(code_buf.as_slice());
        let status = Status::parse(&mut code).unwrap();
        assert!(!status.is_success());
        assert!(status.is_fatal());
    }

    #[test]
    fn test_usb_error_status() {
        let mut code_buf = vec![0; 2];

        code_buf.as_mut_slice().write_u16::<LE>(0xA001).unwrap();
        let mut code = Cursor::new(code_buf.as_slice());
        let status = Status::parse(&mut code).unwrap();
        assert!(!status.is_success());
        assert!(status.is_fatal());
        match status.kind {
            StatusKind::UsbSpecific(..) => {}
            _ => panic!("must be USB specific error status"),
        }
    }
}