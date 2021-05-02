// SPDX-License-Identifier: GPL-2.0

use crate::bindings;
use crate::c_types;
use crate::CStr;
use crate::error::{Error, KernelResult};

pub struct SpiDevice(*mut bindings::spi_device);

impl SpiDevice {
    pub fn from_ptr(dev: *mut bindings::spi_device) -> Self {
        SpiDevice(dev)
    }
}

pub struct DriverRegistration {
    this_module: &'static crate::ThisModule,
    name: CStr<'static>,
    probe: Option<SpiMethod>,
    remove: Option<SpiMethod>,
    shutdown: Option<SpiMethodVoid>,
    spi_driver: Option<bindings::spi_driver>,
}

impl DriverRegistration {
    pub fn new(this_module: &'static crate::ThisModule, name: CStr<'static>) -> Self {
        DriverRegistration {
            this_module,
            name,
            probe: None,
            remove: None,
            shutdown: None,
            spi_driver: None,
        }
    }

    pub fn with_probe(mut self, func: SpiMethod) -> Self {
        self.probe = Some(func);
        self
    } // FIXME: Add remove and shutdown

    pub fn register(&mut self) -> KernelResult {
        let mut spi_driver = bindings::spi_driver::default();
        spi_driver.driver.name = self.name.as_ptr() as *const c_types::c_char;
        spi_driver.probe = self.probe;
        spi_driver.remove = self.remove;
        spi_driver.shutdown = self.shutdown;

        self.spi_driver = Some(spi_driver);

        let res = unsafe {
            bindings::__spi_register_driver(
                self.this_module.0,
                &mut spi_driver,
            )
        };

        match res {
            0 => Ok(()),
            _ => Err(Error::from_kernel_errno(res)),
        }
    }
}

impl Drop for DriverRegistration {
    fn drop(&mut self) {
        unsafe { bindings::driver_unregister(&mut self.spi_driver.unwrap().driver) } // FIXME: No unwrap? But it's safe?
    }
}

type SpiMethod = unsafe extern "C" fn(*mut bindings::spi_device) -> c_types::c_int;
type SpiMethodVoid = unsafe extern "C" fn(*mut bindings::spi_device) -> ();

#[macro_export]
macro_rules! spi_method {
    (fn $method_name:ident ($device_name:ident : SpiDevice) -> KernelResult $block:block) => {
        unsafe extern "C" fn $method_name(dev: *mut kernel::bindings::spi_device) -> kernel::c_types::c_int {
            use kernel::spi::SpiDevice;

            fn inner($device_name: SpiDevice) -> KernelResult $block

            match inner(SpiDevice::from_ptr(dev)) {
                Ok(_) => 0,
                Err(e) => e.to_kernel_errno(),
            }
        }
    };
    (fn $method_name:ident ($device_name:ident : SpiDevice) $block:block) => {
        unsafe extern "C" fn $method_name(dev: *mut kernel::bindings::spi_device) {
            use kernel::spi::SpiDevice;

            fn inner($device_name: SpiDevice) $block

            inner(SpiDevice::from_ptr(dev))
        }
    };
}
