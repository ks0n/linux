// SPDX-License-Identifier: GPL-2.0

//! This module provides safer and higher level abstraction over the kernel's SPI types
//! and functions.
//!
//! C header: [`include/linux/spi/spi.h`](../../../../include/linux/spi/spi.h)

use crate::{
    prelude::*,
    bindings,
    ThisModule,
    error::{code::*, to_result, Error, Result, VTABLE_DEFAULT_ERROR},
    str::CStr,
    device_id::{IdTable, RawDeviceId},
    static_assert,
    driver,
    build_error,
};
use core::mem::size_of;

pub struct Adapter<T: Driver>(T);

impl<T: Driver> driver::DriverOps for Adapter<T> {
    type RegType = bindings::spi_driver;

    fn register(
        sdrv: &mut Self::RegType,
        name: &'static CStr,
        module: &'static ThisModule,
    ) -> Result {
        sdrv.driver.name = name.as_char_ptr();
        sdrv.probe = Some(Self::probe_callback);
        if <T as Driver>::HAS_REMOVE {
            sdrv.remove = Some(Self::remove_callback);
        }
        if <T as Driver>::HAS_SHUTDOWN {
            sdrv.shutdown = Some(Self::shutdown_callback);
        }
        sdrv.id_table = if let Some(table) = T::ID_TABLE {
            table.as_ref()
        } else {
            // TODO: Can this path be valid? In C you can have an empty id_table but for now in Rust it's
            // mandatory.
            unreachable!("id_table should always contain something");
        };

        to_result(unsafe {
            bindings::__spi_register_driver(module.0, sdrv)
        })
    }

    fn unregister(reg: &mut Self::RegType) {
        unsafe { bindings::driver_unregister(&mut reg.driver) }
    }
}

impl<T: Driver> Adapter<T> {
    extern "C" fn probe_callback(spi: *mut bindings::spi_device) -> core::ffi::c_int {
        // SAFETY: Safe because the core kernel only ever calls the probe callback with a valid
        // `spi`.
        let mut dev = unsafe { SpiDevice::from_ptr(spi) };

        match T::probe(&mut dev) {
            Ok(_) => 0,
            Err(err) => Error::to_errno(err),
        }
    }

    extern "C" fn remove_callback(spi: *mut bindings::spi_device) {
        // SAFETY: Safe because the core kernel only ever calls the probe callback with a valid
        // `spi`.
        let mut dev = unsafe { SpiDevice::from_ptr(spi) };

        T::remove(&mut dev);
    }

    extern "C" fn shutdown_callback(spi: *mut bindings::spi_device) {
        // SAFETY: Safe because the core kernel only ever calls the probe callback with a valid
        // `spi`.
        let mut dev = unsafe { SpiDevice::from_ptr(spi) };

        T::shutdown(&mut dev);
    }
}

/// Declares a kernel module that exposes a SPI driver.
///
/// # Example
///
///```ignore
/// kernel::module_spi_driver! {
///     type: MyDriver,
///     name: "Module name",
///     author: "Author name",
///     description: "Description",
///     license: "GPL v2",
/// }
///```
#[macro_export]
macro_rules! module_spi_driver {
    ($($f:tt)*) => {
        $crate::module_driver!(<T>, $crate::spi::Adapter<T>, { $($f)* });
    };
}

#[vtable]
pub trait Driver {
    type Data;

    const ID_TABLE: Option<IdTable<'static, SpiDeviceId<Self::Data>, ()>>;

    /// Corresponds to the kernel's `spi_driver`'s `probe` method field.
    fn probe(_spi_dev: &mut SpiDevice) -> Result<i32>;

    /// Corresponds to the kernel's `spi_driver`'s `remove` method field.
    fn remove(_spi_dev: &mut SpiDevice) {
        build_error(VTABLE_DEFAULT_ERROR);
    }

    /// Corresponds to the kernel's `spi_driver`'s `shutdown` method field.
    fn shutdown(_spi_dev: &mut SpiDevice) {
        build_error(VTABLE_DEFAULT_ERROR);
    }
}

/// Wrapper struct around the kernel's `spi_device`.
#[derive(Clone, Copy)]
pub struct SpiDevice(*mut bindings::spi_device);

impl SpiDevice {
    /// Create an [`SpiDevice`] from a mutable spi_device raw pointer. This function is unsafe
    /// as the pointer might be invalid.
    ///
    /// The pointer must be valid.
    ///
    /// You probably do not want to use this abstraction directly. It is mainly used
    /// by this abstraction to wrap valid pointers given by the Kernel to the different
    /// SPI methods: `probe`, `remove` and `shutdown`.
    pub unsafe fn from_ptr(dev: *mut bindings::spi_device) -> Self {
        SpiDevice(dev)
    }

    // /// Access the raw pointer from an [`SpiDevice`] instance.
    pub fn to_ptr(&mut self) -> *mut bindings::spi_device {
        self.0
    }
}

/// Wrapper struct around the kernel's `struct spi_device_id`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SpiDeviceId<T> {
    name: [i8; bindings::SPI_NAME_SIZE as usize],
    driver_data: T,
}

impl<T> SpiDeviceId<T> {
    pub const fn new(name: &CStr, driver_data: T) -> Self {
        build_assert!(size_of::<T>() == size_of::<bindings::__kernel_ulong_t>());

        let len = name.len_with_nul();

        if len > bindings::SPI_NAME_SIZE as usize {
            build_error!("WTF");
        }

        let name = name.as_bytes_with_nul();
        let mut name_array: [i8; bindings::SPI_NAME_SIZE as usize] = [0; bindings::SPI_NAME_SIZE as usize];

        let mut i = 0;
        while i < len {
            name_array[i] = name[i] as i8;
            i += 1;
        }

        SpiDeviceId {
            name: name_array,
            driver_data,
        }
    }

    pub const fn to_rawid(&self, _offset: isize) -> bindings::spi_device_id {
        bindings::spi_device_id {
            name: self.name,
            driver_data: unsafe { core::mem::transmute_copy::<T, bindings::__kernel_ulong_t>(&self.driver_data) },
        }
    }
}

unsafe impl<T> RawDeviceId for SpiDeviceId<T> {
    type RawType = bindings::spi_device_id;

    const ZERO: Self::RawType = bindings::spi_device_id {
        name: [0; bindings::SPI_NAME_SIZE as usize],
        driver_data: 0
    };
}

}
