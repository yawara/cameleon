/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! This module contains libusb async api wrapper without any overhead.
//! NEVER make this module public because all functions in this module may cause UB if
//! preconditions are not followed.
// The implementation in the module is written with heavily reference to
// https://github.com/kevinmehall/rusb/blob/km-pipe-approach/src/device_handle/async_api.rs.

use std::{
    collections::VecDeque,
    convert::TryInto,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering::SeqCst},
    time::{Duration, Instant},
};

use cameleon_device::u3v::ReceiveChannel;
use rusb::UsbContext;
use thiserror::Error;

use crate::{StreamError, StreamResult};

/// Represents a pool of asynchronous transfers, that can be polled to completion.
pub(super) struct AsyncPool<'a> {
    device: &'a ReceiveChannel,
    pending: VecDeque<AsyncTransfer>,
}

impl<'a> AsyncPool<'a> {
    pub(super) fn new(device: &'a ReceiveChannel) -> Self {
        Self {
            device,
            pending: VecDeque::new(),
        }
    }

    pub(super) fn submit(&mut self, buf: &mut [u8]) -> StreamResult<()> {
        // Safety: If transfer is submitted, it is pushed onto `pending` where it will be
        // dropped before `device` is freed.
        unsafe {
            let mut transfer = AsyncTransfer::new_bulk(
                self.device.device_handle.as_raw(),
                self.device.iface_info.bulk_in_ep,
                buf,
            );
            transfer.submit()?;
            self.pending.push_back(transfer);
            Ok(())
        }
    }

    pub(super) fn poll(&mut self, timeout: Duration) -> StreamResult<usize> {
        let next = self.pending.front().ok_or(AsyncError::NoTransfersPending)?;
        if poll_completed(
            self.device.device_handle.context(),
            timeout,
            next.completed_flag(),
        )? {
            let mut transfer = self.pending.pop_front().unwrap();
            Ok(transfer.handle_completed()?)
        } else {
            Err(AsyncError::Timeout.into())
        }
    }

    pub(super) fn cancel_all(&mut self) {
        // Cancel in reverse order to avoid a race condition in which one
        // transfer is cancelled but another submitted later makes its way onto
        // the bus.
        for transfer in self.pending.iter_mut().rev() {
            transfer.cancel();
        }
    }

    /// Returns the number of async transfers pending.
    pub(super) fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Returns `true` if there is no pending transfer.
    pub(super) fn is_empty(&self) -> bool {
        self.pending() == 0
    }
}

impl<'a> Drop for AsyncPool<'a> {
    fn drop(&mut self) {
        self.cancel_all();
        while !self.is_empty() {
            self.poll(Duration::from_secs(1)).ok();
        }
    }
}

struct AsyncTransfer {
    ptr: NonNull<libusb1_sys::libusb_transfer>,
}

impl AsyncTransfer {
    /// Invariant: Caller must ensure `device` outlives this transfer.
    unsafe fn new_bulk(
        device: *mut libusb1_sys::libusb_device_handle,
        endpoint: u8,
        buffer: &mut [u8],
    ) -> Self {
        // non-isochronous endpoints (e.g. control, bulk, interrupt) specify a value of 0
        // This is step 1 of async API
        let ptr = libusb1_sys::libusb_alloc_transfer(0);
        let ptr = NonNull::new(ptr).expect("Could not allocate transfer!");

        let user_data = Box::into_raw(Box::new(AtomicBool::new(false))).cast::<libc::c_void>();

        let length = buffer.len() as libc::c_int;

        libusb1_sys::libusb_fill_bulk_transfer(
            ptr.as_ptr(),
            device,
            endpoint,
            buffer.as_ptr() as *mut u8,
            length,
            Self::transfer_cb,
            user_data,
            0,
        );

        Self { ptr }
    }

    //// Part of step 4 of async API the transfer is finished being handled when
    //// `poll()` is called.
    extern "system" fn transfer_cb(transfer: *mut libusb1_sys::libusb_transfer) {
        // Safety: transfer is still valid because libusb just completed
        // it but we haven't told anyone yet. user_data remains valid
        // because it is freed only with the transfer.
        // After the store to completed, these may no longer be valid if
        // the polling thread freed it after seeing it completed.
        let completed = unsafe {
            let transfer = &mut *transfer;
            &*transfer.user_data.cast::<AtomicBool>()
        };
        completed.store(true, SeqCst);
    }

    fn transfer(&self) -> &libusb1_sys::libusb_transfer {
        // Safety: transfer remains valid as long as self
        unsafe { self.ptr.as_ref() }
    }

    fn completed_flag(&self) -> &AtomicBool {
        // Safety: transfer and user_data remain valid as long as self
        unsafe { &*self.transfer().user_data.cast::<AtomicBool>() }
    }

    // Step 3 of async API
    fn submit(&mut self) -> StreamResult<()> {
        self.completed_flag().store(false, SeqCst);
        let errno = unsafe { libusb1_sys::libusb_submit_transfer(self.ptr.as_ptr()) };
        Ok(AsyncError::from_libusb_error(errno)?)
    }

    fn cancel(&mut self) {
        unsafe {
            libusb1_sys::libusb_cancel_transfer(self.ptr.as_ptr());
        }
    }

    fn handle_completed(&mut self) -> StreamResult<usize> {
        assert!(self
            .completed_flag()
            .load(std::sync::atomic::Ordering::Relaxed));
        use libusb1_sys::constants::*;
        let err = match self.transfer().status {
            LIBUSB_TRANSFER_COMPLETED => {
                let transfer = self.transfer();
                debug_assert!(transfer.length >= transfer.actual_length);
                return Ok(transfer.actual_length as usize);
            }
            LIBUSB_TRANSFER_CANCELLED => AsyncError::Cancelled,
            LIBUSB_TRANSFER_ERROR => AsyncError::Other,
            LIBUSB_TRANSFER_TIMED_OUT => {
                unreachable!("We are using timeout=0 which means no timeout")
            }
            LIBUSB_TRANSFER_STALL => AsyncError::Stall,
            LIBUSB_TRANSFER_NO_DEVICE => AsyncError::Disconnected,
            LIBUSB_TRANSFER_OVERFLOW => AsyncError::Overflow,
            _ => unreachable!(),
        };
        Err(err.into())
    }
}

/// Invariant: transfer must not be pending.
impl Drop for AsyncTransfer {
    fn drop(&mut self) {
        unsafe {
            libusb1_sys::libusb_free_transfer(self.ptr.as_ptr());
        }
    }
}

/// This is effectively libusb_handle_events_timeout_completed, but with
/// `completed` as `AtomicBool` instead of `c_int` so it is safe to access
/// without the events lock held. It also continues polling until completion,
/// timeout, or error, instead of potentially returning early.
///
/// This design is based on
/// https://libusb.sourceforge.io/api-1.0/libusb_mtasync.html#threadwait
fn poll_completed(
    ctx: &impl UsbContext,
    timeout: Duration,
    completed: &AtomicBool,
) -> StreamResult<bool> {
    use libusb1_sys::{constants::*, *};

    let deadline = Instant::now() + timeout;

    unsafe {
        let mut err = 0;
        while err == 0 && !completed.load(SeqCst) && deadline > Instant::now() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeval = libc::timeval {
                tv_sec: remaining.as_secs().try_into().unwrap(),
                tv_usec: remaining.subsec_micros().try_into().unwrap(),
            };

            if libusb_try_lock_events(ctx.as_raw()) == 0 {
                if !completed.load(SeqCst) && libusb_event_handling_ok(ctx.as_raw()) != 0 {
                    err = libusb_handle_events_locked(ctx.as_raw(), &timeval as *const _);
                }
                libusb_unlock_events(ctx.as_raw());
            } else {
                libusb_lock_event_waiters(ctx.as_raw());
                if !completed.load(SeqCst) && libusb_event_handler_active(ctx.as_raw()) != 0 {
                    libusb_wait_for_event(ctx.as_raw(), &timeval as *const _);
                }
                libusb_unlock_event_waiters(ctx.as_raw());
            }
        }

        match err {
            0 => Ok(completed.load(SeqCst)),
            LIBUSB_ERROR_TIMEOUT => Ok(false),
            e => Err(AsyncError::from_libusb_error(e).unwrap_err().into()),
        }
    }
}

#[derive(Error, Debug)]
enum AsyncError {
    #[error("no transfers pending")]
    NoTransfersPending,
    #[error("transfer is stalled")]
    Stall,
    #[error("device was disconnected")]
    Disconnected,
    #[error("transfer was cancelled")]
    Cancelled,
    #[error("input/output error")]
    Io,
    #[error("invalid parameter")]
    InvalidParam,
    #[error("access denied (insufficient permissions)")]
    Access,
    #[error("no such device (it may have been disconnected)")]
    NoDevice,
    #[error("entity not found")]
    NotFound,
    #[error("resource busy")]
    Busy,
    #[error("operation timed out")]
    Timeout,
    #[error("overflow")]
    Overflow,
    #[error("pipe error")]
    Pipe,
    #[error("system call interrupted (perhaps due to signal)")]
    Interrupted,
    #[error("insufficient memory")]
    NoMem,
    #[error("operation not supported or unimplemented on this platform")]
    NotSupported,
    #[error("other error")]
    Other,
}

impl AsyncError {
    fn from_libusb_error(err: i32) -> Result<(), Self> {
        match err {
            0 => Ok(()),
            libusb1_sys::constants::LIBUSB_ERROR_IO => Err(Self::Io),
            libusb1_sys::constants::LIBUSB_ERROR_INVALID_PARAM => Err(Self::InvalidParam),
            libusb1_sys::constants::LIBUSB_ERROR_ACCESS => Err(Self::Access),
            libusb1_sys::constants::LIBUSB_ERROR_NO_DEVICE => Err(Self::NoDevice),
            libusb1_sys::constants::LIBUSB_ERROR_NOT_FOUND => Err(Self::NotFound),
            libusb1_sys::constants::LIBUSB_ERROR_BUSY => Err(Self::Busy),
            libusb1_sys::constants::LIBUSB_ERROR_TIMEOUT => Err(Self::Timeout),
            libusb1_sys::constants::LIBUSB_ERROR_OVERFLOW => Err(Self::Overflow),
            libusb1_sys::constants::LIBUSB_ERROR_PIPE => Err(Self::Pipe),
            libusb1_sys::constants::LIBUSB_ERROR_INTERRUPTED => Err(Self::Interrupted),
            libusb1_sys::constants::LIBUSB_ERROR_NO_MEM => Err(Self::NoMem),
            libusb1_sys::constants::LIBUSB_ERROR_NOT_SUPPORTED => Err(Self::NotSupported),
            libusb1_sys::constants::LIBUSB_ERROR_OTHER => Err(Self::Other),
            _ => unreachable!(),
        }
    }
}

impl From<AsyncError> for StreamError {
    fn from(err: AsyncError) -> Self {
        match err {
            AsyncError::Disconnected => Self::Disconnected,
            _ => StreamError::Io(err.into()),
        }
    }
}
