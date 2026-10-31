//! Desktop configuration schema and read-only migration of legacy flash records.
//! sequential-storage owns journaling, CRC, recovery and garbage collection.
use crate::keyboard::{self, Keymap};
use heapless::{String, Vec};

pub const LEGACY_RECORD: usize = 720;
pub const KV_KEY: u8 = 1;
pub const KV_START: u32 = 0xff2000;
pub const KV_END: u32 = 0x1000000;
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WifiProfile {
    pub ssid: String<32>,
    pub password: String<64>,
}
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub brightness: u8,
    pub volume: u8,
    pub ble_advertising: bool,
    pub ble_name: String<24>,
    pub ssid: String<32>,
    pub password: String<64>,
    pub wifi_enabled: bool,
    pub profiles: Vec<WifiProfile, 8>,
    #[serde(skip)]
    pub revision: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            brightness: 100,
            volume: 100,
            ble_advertising: false,
            ble_name: String::try_from("P4 Desktop").unwrap(),
            ssid: String::new(),
            password: String::new(),
            wifi_enabled: true,
            profiles: Vec::new(),
            revision: 0,
        }
    }
}
impl Settings {
    /// A single KV value commits all settings and VIA together; the library
    /// provides per-key atomicity. Postcard avoids a handwritten binary layout.
    pub fn decode_kv(bytes: &[u8]) -> Option<(Keymap, Self)> {
        // Postcard structs are positional: explicitly migrate v1 before adding fields.
        let (version, _): (u8, _) = postcard::take_from_bytes(bytes).ok()?;
        let (settings, keys) = if version == 1 {
            let (_, (brightness, volume, ble_advertising, ble_name, ssid, password), keys): (
                u8,
                (u8, u8, bool, String<24>, String<32>, String<64>),
                &[u8],
            ) = postcard::from_bytes(bytes).ok()?;
            let mut settings = Self {
                brightness,
                volume,
                ble_advertising,
                ble_name,
                ssid,
                password,
                ..Self::default()
            };
            if !settings.ssid.is_empty() {
                settings
                    .profiles
                    .push(WifiProfile {
                        ssid: settings.ssid.clone(),
                        password: settings.password.clone(),
                    })
                    .ok()?;
            }
            (settings, keys)
        } else {
            let (_, settings, keys): (u8, Self, &[u8]) = postcard::from_bytes(bytes).ok()?;
            (settings, keys)
        };
        if !matches!(version, 1 | 2)
            || keys.len() != keyboard::PAYLOAD
            || !(5..=100).contains(&settings.brightness)
            || settings.volume > 100
            || settings.ble_name.is_empty()
            || settings.ble_name.chars().any(char::is_control)
        {
            return None;
        }
        let mut map = Keymap::default();
        map.bytes.copy_from_slice(keys);
        Some((map, settings))
    }
    pub fn restore_legacy(record: &[u8; LEGACY_RECORD]) -> Option<(Keymap, Self, u32)> {
        // Migrate existing VIA records without discarding the user's keymap.
        if &record[..8] == b"P4KEY001" {
            let (map, seq) = Keymap::restore(record[..keyboard::RECORD].try_into().ok()?)?;
            return Some((map, Self::default(), seq));
        }
        let generation = u32::from_le_bytes(record[8..12].try_into().ok()?);
        if &record[..8] != b"P4CFG001"
            || u32::from_le_bytes(record[12..16].try_into().ok()?)
                != keyboard::crc(&record[16..]) ^ generation
            || !(5..=100).contains(&record[592])
            || record[593] > 100
            || record[594] > 1
            || record[595] > 32
            || record[596] > 64
            || !(1..=24).contains(&record[597])
        {
            return None;
        }
        let mut settings = Self::default();
        settings.brightness = record[592];
        settings.volume = record[593];
        settings.ble_advertising = record[594] != 0;
        settings.ssid =
            String::try_from(core::str::from_utf8(&record[598..598 + record[595] as usize]).ok()?)
                .ok()?;
        settings.password =
            String::try_from(core::str::from_utf8(&record[630..630 + record[596] as usize]).ok()?)
                .ok()?;
        settings.ble_name =
            String::try_from(core::str::from_utf8(&record[694..694 + record[597] as usize]).ok()?)
                .ok()?;
        if !settings.ssid.is_empty() {
            settings
                .profiles
                .push(WifiProfile {
                    ssid: settings.ssid.clone(),
                    password: settings.password.clone(),
                })
                .ok()?;
        }
        let mut map = Keymap::default();
        map.bytes.copy_from_slice(&record[16..592]);
        Some((map, settings, generation))
    }
}
