// SPDX-License-Identifier: GPL-2.0
// Copyright 2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use efiloader::runtimeservices::GetTime;
use efiloader::runtimeservices::{Time, TimeCapabilities};
use efiloader::status::Status;
use efiloader::status::Status::EFI_SUCCESS;

use core::mem::MaybeUninit;
use mmio::{Allow, VolBox};

//const EFI_TIME_ADJUST_DAYLIGHT: u8 = 0x1;
//const EFI_TIME_IN_DAYLIGHT: u8 = 0x2;

const EFI_UNSPECIFIED_TIMEZONE: u16 = 0x07ff;

#[link_section = ".rtdata"]
static mut _RTC: MaybeUninit<VolBox<u32, Allow, Allow>> = MaybeUninit::uninit();

fn time_from_ts(ts: u32) -> Time {
    let secs = ts % 86400;

    let h = secs / 3600;
    let m = (secs / 60) % 60;
    let s = secs % 60;

    /* algorithm taken from drivers/rtc/lib.c in Linux */
    let (century, day_of_century) = {
        let d = ts / 86400 + 719468;
        let t = 4 * d + 3;

        (t / 146097, t % 146097 / 4)
    };

    let (year_of_century, day_of_year) = {
        let t = 2939745u64 * (4 * day_of_century as u64 + 3);

        (t >> 32, (t & u32::MAX as u64) as u32 / 2939745 / 4)
    };

    let (day, year, month) = {
        let t = 2141 * day_of_year + 132377;
        let y = 100 * century + year_of_century as u32;
        let m = t >> 16;
        let d = (t & u16::MAX as u32) / 2141;
        if m > 12 {
            (d + 1, y + 1, m - 11)
        } else {
            (d + 1, y, m + 1)
        }
    };

    Time {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        hour: h as u8,
        minute: m as u8,
        second: s as u8,
        pad1: 0,
        nanosecond: 0,
        timezone: EFI_UNSPECIFIED_TIMEZONE,
        daylight: 0,
        pad2: 0,
    }
}

extern "efiapi" fn get_time(time: *mut Time, _capabilities: *mut TimeCapabilities) -> Status {
    // Safe because get_time() is never exposed unless _RTC has been written
    let rtc = unsafe { _RTC.assume_init_ref() };
    let t = time_from_ts(rtc.read());

    unsafe {
        *time = t;
    }
    EFI_SUCCESS
}

pub fn init(base: usize) -> GetTime {
    let rtc = unsafe { _RTC.write(VolBox::<u32, Allow, Allow>::new(base as *mut u32)) };

    log::trace!("{:?}\n", time_from_ts(rtc.read()));
    get_time
}
