// SPDX-License-Identifier: GPL-2.0

use crate::bindings;
use crate::c_types;
use crate::CStr;
use crate::error::{Error, KernelResult};

type ProbeMethod = unsafe extern "C" fn(*mut bindings::spi_device) -> i32;
type RemoveMethod = ProbeMethod;
type ShutdownMethod = unsafe extern "C" fn (*mut bindings::spi_device) -> ();
// FIXME: Rustify this

pub struct Registration {
    this_module: &'static crate::ThisModule,
    name: CStr<'static>,
    probe: Option<ProbeMethod>,
    remove: Option<RemoveMethod>,
    shutdown: Option<ShutdownMethod>,
    spi_driver: Option<bindings::spi_driver>,
}

impl Registration {
    pub fn new(this_module: &'static crate::ThisModule, name: CStr<'static>) -> Self { // hspi_driver: bindings::spi_driver) -> Self {
        Registration {
            this_module,
            name,
            probe: None,
            remove: None,
            shutdown: None,
            spi_driver: None,
        }
    }

    pub fn with_probe(mut self, func: ProbeMethod) -> Self {
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

impl Drop for Registration {
    fn drop(&mut self) {
        unsafe { bindings::driver_unregister(&mut self.spi_driver.unwrap().driver) } // FIXME: No unwrap? But it's safe?
    }
}
