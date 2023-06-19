// SPDX-License-Identifier: GPL-2.0

use kernel::c_str;
use kernel::prelude::*;
use kernel::module_spi_driver;
use kernel::define_id_table;
use kernel::spi;
use kernel::spi::{SpiDevice, SpiDeviceId};

module_spi_driver! {
    type: SpiDummy,
    name: "rust_spi_dummy",
    author: "ks0n",
    description: "SPI Dummy Driver",
    license: "GPL",
}

struct SpiDummy;

#[vtable]
impl spi::Driver for SpiDummy {
    type Data = usize;

    define_id_table!(ID_TABLE, SpiDeviceId<<SpiDummy as spi::Driver>::Data>, (), [
        (SpiDeviceId::new(c_str!("SpiDummy"), 42usize), None),
    ]);

    fn probe(spi: &mut SpiDevice) -> Result<i32> {
        pr_info!("[SPI-RS] probed\n");

        Ok(0)
    }
}
