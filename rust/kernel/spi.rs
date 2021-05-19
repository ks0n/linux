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

pub struct ToUse {
    pub probe: bool,
    pub remove: bool,
    pub shutdown: bool,
}

pub const USE_NONE: ToUse = ToUse {
    probe: false,
    remove: false,
    shutdown: false,
};

pub trait SpiMethods {
    const TO_USE: ToUse;

    fn probe(mut _spi_dev: SpiDevice) -> Result {
        Ok(())
    }

    fn remove(mut _spi_dev: SpiDevice) -> Result {
        Ok(())
    }

    fn shutdown(mut _spi_dev: SpiDevice) {}
}

/// Populate the TO_USE field in the `SpiMethods` implementer
///
/// ```rust
/// impl SpiMethods for MySpiMethods {
///     /// Let's say you only want a probe and remove method, no shutdown
///     declare_spi_methods!(probe, remove);
///
///     /// Define your probe and remove methods. If you don't, default implementations
///     /// will be used instead. These default implementations do NOT correspond to the
///     /// kernel's default implementations! If you wish to use the Kernel's default
///     /// spi functions implementations, do not declare them using the `declare_spi_methods`
///     /// macro. For example, here our Driver will use the Kernel's shutdown method.
///     fn probe(spi_dev: SpiDevice) -> Result {
///         // ...
///
///         Ok(())
///     }
///
///     fn remove(spi_dev: SpiDevice) -> Result {
///         // ...
///
///         Ok(())
///     }
/// }
/// ```
#[macro_export]
macro_rules! declare_spi_methods {
    () => {
        const TO_USE: $crate::spi::ToUse = $crate::spi::USE_NONE;
    };
    ($($method:ident),+) => {
        const TO_USE: $crate::spi::ToUse = $crate::spi::ToUse {
            $($method: true),+,
            ..$crate::spi::USE_NONE
        };
    };
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
    pub fn new_pinned<T: SpiMethods>(
        this_module: &'static crate::ThisModule,
        name: &'static CStr,
    ) -> Result<Pin<Box<Self>>> {
        let mut registration = Pin::from(Box::try_new(Self::new(this_module, name))?);

        registration.as_mut().register::<T>()?;

        Ok(registration)
    }

    unsafe extern "C" fn probe_wrapper<T: SpiMethods>(
        spi_dev: *mut bindings::spi_device,
    ) -> c_types::c_int {
        // SAFETY: The spi_dev pointer is provided by the kernel and is sure to be valid
        match T::probe(unsafe{SpiDevice::from_ptr(spi_dev)}) {
            Ok(_) => 0,
            Err(e) => e.to_kernel_errno(),
        }
    }

    unsafe extern "C" fn remove_wrapper<T: SpiMethods>(
        spi_dev: *mut bindings::spi_device,
    ) -> c_types::c_int {
        // SAFETY: The spi_dev pointer is provided by the kernel and is sure to be valid
        match T::remove(unsafe {SpiDevice::from_ptr(spi_dev)}) {
            Ok(_) => 0,
            Err(e) => e.to_kernel_errno(),
        }
    }

    unsafe extern "C" fn shutdown_wrapper<T: SpiMethods>(spi_dev: *mut bindings::spi_device) {
        // SAFETY: The spi_dev pointer is provided by the kernel and is sure to be valid
        T::shutdown(unsafe {SpiDevice::from_ptr(spi_dev)})
    }

    // FIXME: Add documentation
    pub fn register<T: SpiMethods>(self: Pin<&mut Self>) -> Result {
        let this = unsafe { self.get_unchecked_mut() };
        if this.registered {
            return Err(Error::EINVAL);
        }

        this.spi_driver.driver.name = this.name.as_ptr() as *const c_types::c_char;
        this.spi_driver.probe = T::TO_USE
            .probe
            .then(|| DriverRegistration::probe_wrapper::<T> as _);
        this.spi_driver.remove = T::TO_USE
            .remove
            .then(|| DriverRegistration::remove_wrapper::<T> as _);
        this.spi_driver.shutdown = T::TO_USE
            .shutdown
            .then(|| DriverRegistration::shutdown_wrapper::<T> as _);

        let res =
            unsafe { bindings::__spi_register_driver(this.this_module.0, &mut this.spi_driver) };

        if res != 0 {
            return Err(Error::from_kernel_errno(res));
        }

        this.registered = true;
        Ok(())
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

// SAFETY: All functions work from any thread.
unsafe impl Send for DriverRegistration {}

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
