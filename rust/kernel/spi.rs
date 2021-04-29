// SPDX-License-Identifier: GPL-2.0

use crate::bindings;
use crate::c_types;
use crate::error::{Error, Result};
use crate::str::CStr;
use alloc::boxed::Box;
use core::pin::Pin;

#[derive(Clone, Copy)]
pub struct SpiDevice(*mut bindings::spi_device);

impl SpiDevice {
    pub unsafe fn from_ptr(dev: *mut bindings::spi_device) -> Self {
        SpiDevice(dev)
    }

    pub fn to_ptr(&mut self) -> *mut bindings::spi_device {
        self.0
    }
}

pub struct DriverRegistration {
    this_module: &'static crate::ThisModule,
    registered: bool,
    name: &'static CStr,
    spi_driver: bindings::spi_driver,
}

impl DriverRegistration {
    fn new(this_module: &'static crate::ThisModule, name: &'static CStr) -> Self {
        DriverRegistration {
            this_module,
            name,
            registered: false,
            spi_driver: bindings::spi_driver::default(),
        }
    }

    // FIXME: Add documentation
    pub fn new_pinned(
        this_module: &'static crate::ThisModule,
        name: &'static CStr,
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
        let mut spi_driver = bindings::spi_driver::default();
        spi_driver.driver.name = self.name.as_ptr() as *const c_types::c_char;
        spi_driver.probe = self.probe;
        spi_driver.remove = self.remove;
        spi_driver.shutdown = self.shutdown;

        let this = unsafe { self.get_unchecked_mut() };
        if this.registered {
            return Err(Error::EINVAL);
        }

        this.spi_driver = Some(spi_driver);

        let res = unsafe {
            bindings::__spi_register_driver(this.this_module.0, this.spi_driver.as_mut().unwrap())
        };

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
        unsafe { bindings::driver_unregister(&mut self.spi_driver.as_mut().unwrap().driver) }
        // FIXME: No unwrap? But it's safe?
    }
}

// FIXME: Fix SAFETY documentation

// SAFETY: The only method is `register()`, which requires a (pinned) mutable `Registration`, so it
// is safe to pass `&Registration` to multiple threads because it offers no interior mutability.
unsafe impl Sync for DriverRegistration {}

// SAFETY: All functions work from any thread.
unsafe impl Send for DriverRegistration {}

type SpiMethod = unsafe extern "C" fn(*mut bindings::spi_device) -> c_types::c_int;
type SpiMethodVoid = unsafe extern "C" fn(*mut bindings::spi_device) -> ();

#[macro_export]
macro_rules! spi_method {
    (fn $method_name:ident (mut $device_name:ident : SpiDevice) -> Result $block:block) => {
        unsafe extern "C" fn $method_name(dev: *mut kernel::bindings::spi_device) -> kernel::c_types::c_int {
            use kernel::spi::SpiDevice;

            fn inner(mut $device_name: SpiDevice) -> Result $block

            // SAFETY: The dev pointer is provided by the kernel and is sure to be valid
            match inner(unsafe { SpiDevice::from_ptr(dev) }) {
                Ok(_) => 0,
                Err(e) => e.to_kernel_errno(),
            }
        }
    };
    (fn $method_name:ident (mut $device_name:ident : SpiDevice) $block:block) => {
        unsafe extern "C" fn $method_name(dev: *mut kernel::bindings::spi_device) {
            use kernel::spi::SpiDevice;

            fn inner(mut $device_name: SpiDevice) $block

            // SAFETY: The dev pointer is provided by the kernel and is sure to be valid
            inner(unsafe { SpiDevice::from_ptr(dev) })
        }
    };
}

pub struct Spi;

impl Spi {
    pub fn write_then_read(dev: &mut SpiDevice, tx_buf: &[u8], rx_buf: &mut [u8]) -> Result {
        let res = unsafe {
            bindings::spi_write_then_read(
                dev.to_ptr(),
                tx_buf.as_ptr() as *const c_types::c_void,
                tx_buf.len() as c_types::c_uint,
                rx_buf.as_mut_ptr() as *mut c_types::c_void,
                rx_buf.len() as c_types::c_uint,
            )
        };

        match res {
            0 => Ok(()),                               // 0 indicates a valid transfer,
            err => Err(Error::from_kernel_errno(err)), // A negative number indicates an error
        }
    }

    pub fn write(dev: &mut SpiDevice, tx_buf: &[u8]) -> Result {
        Spi::write_then_read(dev, tx_buf, &mut [0u8; 0])
    }

    pub fn read(dev: &mut SpiDevice, rx_buf: &mut [u8]) -> Result {
        Spi::write_then_read(dev, &[0u8; 0], rx_buf)
    }
}
