// SPDX-License-Identifier: GPL-2.0

//! SPI Abstraction API.
//!
//! This module allows the user to register their own SPI Driver, with associated probe,
//! remove and shutdown methods. It also provides a way to call the main SPI Transfer
//! function, spi_write_then_read(), as well as the associated quality-of-life macros,
//! spi_write() and spi_read()

use crate::bindings;
use crate::c_types;
use crate::error::{Error};
use crate::Result as KernelResult; // FIXME: Rework file based on new API
use crate::CStr;
use alloc::boxed::Box;
use core::pin::Pin;

/// Abstraction around an SPI device
#[derive(Clone, Copy)]
pub struct SpiDevice(*mut bindings::spi_device);

impl SpiDevice {
    /// Instanciate an SPI Device from a given, non-null and *valid* spi_device
    pub fn from_ptr(dev: *mut bindings::spi_device) -> Self {
        SpiDevice(dev)
    }

    /// Get the underlying pointer from the SPI Device
    pub fn to_ptr(&mut self) -> *mut bindings::spi_device {
        self.0
    }
}

/// A registration of an SPI driver
pub struct DriverRegistration {
    this_module: &'static crate::ThisModule,
    registered: bool,
    name: CStr<'static>,
    probe: Option<SpiMethod>,
    remove: Option<SpiMethod>,
    shutdown: Option<SpiMethodVoid>,
    spi_driver: Option<bindings::spi_driver>,
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
            spi_driver: None,
        }
    }

    /// Create a new pinned SPI Driver and register it
    pub fn new_pinned(
        this_module: &'static crate::ThisModule,
        name: CStr<'static>,
        probe: Option<SpiMethod>,
        remove: Option<SpiMethod>,
        shutdown: Option<SpiMethodVoid>,
    ) -> KernelResult<Pin<Box<Self>>> {
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

    /// Register a pinned SPI Driver
    pub fn register(self: Pin<&mut Self>) -> KernelResult {
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

        let res = unsafe { bindings::__spi_register_driver(this.this_module.0, &mut spi_driver) };

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
        unsafe { bindings::driver_unregister(&mut self.spi_driver.unwrap().driver) }
        // FIXME: No unwrap? But it's safe?
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

/// Helper macro around the declaration of "SPI methods". There are two types of SPI Methods:
/// The one used by probe() and remove(), which takes an SpiDevice as paramter and return
/// a KernelResult, and the one used by shutdown() which takes an SpiDevice as parameter but
/// does not return anything.
/// The way to declare methods is the following:
/// - Write a function the way you would in Rust
/// - Be careful: The function's signature must be one of the following:
///     - `fn <name>(mut <device_name>: SpiDevice) -> KernelResult` for probe and remove
///     - `fn <name>(mut <device_name>: SpiDevice)` for shutdown
/// - Surround each function declaration with the spi_method! macro to generate correct
/// functions, callable by the kernel.
/// - Remember to pass your functions as parameters when instantiating a new
/// spi::DriverRegistration
#[macro_export]
macro_rules! spi_method {
    // FIXME: Add recipe with compile_error!() to indicate syntax errors to the user
    (fn $method_name:ident (mut $device_name:ident : SpiDevice) -> KernelResult $block:block) => {
        unsafe extern "C" fn $method_name(dev: *mut kernel::bindings::spi_device) -> kernel::c_types::c_int {
            use kernel::spi::SpiDevice;

            fn inner(mut $device_name: SpiDevice) -> KernelResult $block

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

/// Abstraction struct around basic SPI functions: write_then_read, write, read...
pub struct Spi;

impl Spi {
    /// Transfer data on a given SPI device. This corresponds to the kernel's
    /// `spi_write_then_read`
    pub fn write_then_read(
        dev: &mut SpiDevice,
        tx_buf: &[u8],
        n_tx: usize,
        rx_buf: &mut [u8],
        n_rx: usize,
    ) -> KernelResult {
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

    /// Write data to a given SPI device. This corresponds to the kernel's `spi_write`
    #[inline]
    pub fn write(dev: &mut SpiDevice, tx_buf: &[u8], n_tx: usize) -> KernelResult {
        Spi::write_then_read(dev, tx_buf, n_tx, &mut [0u8; 0], 0)
    }

    /// Read data from a given SPI device. This corresponds to the kernel's `spi_read`
    #[inline]
    pub fn read(dev: &mut SpiDevice, rx_buf: &mut [u8], n_rx: usize) -> KernelResult {
        Spi::write_then_read(dev, &[0u8; 0], 0, rx_buf, n_rx)
    }
}
