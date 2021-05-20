// SPDX-License-Identifier: GPL-2.0

use crate::bindings;
use crate::c_types;
use crate::error::{Error, Result};
use crate::CStr;
use alloc::boxed::Box;
use core::pin::Pin;

#[derive(Clone, Copy)]
pub struct SpiDevice(*mut bindings::spi_device);

impl SpiDevice {
    pub fn from_ptr(dev: *mut bindings::spi_device) -> Self {
        SpiDevice(dev)
    }

    pub fn to_ptr(&mut self) -> *mut bindings::spi_device {
        self.0
    }
}

pub struct DriverRegistration {
    this_module: &'static crate::ThisModule,
    registered: bool,
    name: CStr<'static>,
    probe: Option<SpiMethod>,
    remove: Option<SpiMethod>,
    shutdown: Option<SpiMethodVoid>,
    spi_driver: bindings::spi_driver,
}

impl DriverRegistration {
    fn new(
        this_module: &'static crate::ThisModule,
        name: CStr<'static>,
        probe: Option<SpiMethod>,
        remove: Option<SpiMethod>,
        shutdown: Option<SpiMethodVoid>,
    ) -> Self {
        DriverRegistration {
            this_module,
            name,
            registered: false,
            probe,
            remove,
            shutdown,
            spi_driver: bindings::spi_driver::default(),
        }
    }

    // FIXME: Add documentation
    pub fn new_pinned(
        this_module: &'static crate::ThisModule,
        name: CStr<'static>,
        probe: Option<SpiMethod>,
        remove: Option<SpiMethod>,
        shutdown: Option<SpiMethodVoid>,
    ) -> Result<Pin<Box<Self>>> {
        let mut registration = Pin::from(Box::try_new(Self::new(
            this_module,
            name,
            probe,
            remove,
            shutdown,
        ))?);

        registration.as_mut().register()?;

        Ok(registration)
    }

    // FIXME: Add documentation
    pub fn register(self: Pin<&mut Self>) -> Result {
        let this = unsafe { self.get_unchecked_mut() };
        if this.registered {
            return Err(Error::EINVAL);
        }

        this.spi_driver.driver.name = this.name.as_ptr() as *const c_types::c_char;
        this.spi_driver.probe = this.probe;
        this.spi_driver.remove = this.remove;
        this.spi_driver.shutdown = this.shutdown;

        let res =
            unsafe { bindings::__spi_register_driver(this.this_module.0, &mut this.spi_driver) };

        match res {
            0 => {
                this.registered = true;
                Ok(())
            }
            _ => Err(Error::from_kernel_errno(res)),
        }
    }
}

impl Drop for DriverRegistration {
    fn drop(&mut self) {
        unsafe { bindings::driver_unregister(&mut self.spi_driver.driver) }
    }
}

// FIXME: Fix SAFETY documentation

// SAFETY: The only method is `register()`, which requires a (pinned) mutable `Registration`, so it
// is safe to pass `&Registration` to multiple threads because it offers no interior mutability.
unsafe impl Sync for DriverRegistration {}

// SAFETY: The only method is `register()`, which requires a (pinned) mutable `Registration`, so it
// is safe to pass `&Registration` to multiple threads because it offers no interior mutability.
unsafe impl Send for DriverRegistration {}

type SpiMethod = unsafe extern "C" fn(*mut bindings::spi_device) -> c_types::c_int;
type SpiMethodVoid = unsafe extern "C" fn(*mut bindings::spi_device) -> ();

#[macro_export]
macro_rules! spi_method {
    (fn $method_name:ident (mut $device_name:ident : SpiDevice) -> Result $block:block) => {
        unsafe extern "C" fn $method_name(dev: *mut kernel::bindings::spi_device) -> kernel::c_types::c_int {
            use kernel::spi::SpiDevice;

            fn inner(mut $device_name: SpiDevice) -> Result $block

            match inner(SpiDevice::from_ptr(dev)) {
                Ok(_) => 0,
                Err(e) => e.to_kernel_errno(),
            }
        }
    };
    (fn $method_name:ident (mut $device_name:ident : SpiDevice) $block:block) => {
        unsafe extern "C" fn $method_name(dev: *mut kernel::bindings::spi_device) {
            use kernel::spi::SpiDevice;

            fn inner(mut $device_name: SpiDevice) $block

            inner(SpiDevice::from_ptr(dev))
        }
    };
}

pub struct Spi;

impl Spi {
    pub fn write_then_read(
        dev: &mut SpiDevice,
        tx_buf: &[u8],
        n_tx: usize,
        rx_buf: &mut [u8],
        n_rx: usize,
    ) -> Result {
        let res = unsafe {
            bindings::spi_write_then_read(
                dev.to_ptr(),
                tx_buf.as_ptr() as *const c_types::c_void,
                n_tx as c_types::c_uint,
                rx_buf.as_ptr() as *mut c_types::c_void,
                n_rx as c_types::c_uint,
            )
        };

        match res {
            0 => Ok(()),                               // 0 indicates a valid transfer,
            err => Err(Error::from_kernel_errno(err)), // A negative number indicates an error
        }
    }

    #[inline]
    pub fn write(dev: &mut SpiDevice, tx_buf: &[u8], n_tx: usize) -> Result {
        Spi::write_then_read(dev, tx_buf, n_tx, &mut [0u8; 0], 0)
    }

    #[inline]
    pub fn read(dev: &mut SpiDevice, rx_buf: &mut [u8], n_rx: usize) -> Result {
        Spi::write_then_read(dev, &[0u8; 0], 0, rx_buf, n_rx)
    }
}
