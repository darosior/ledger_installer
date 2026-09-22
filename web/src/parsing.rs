//! Parsing utilities for device responses.

/// Parse device version info from APDU response.
///
/// Returns (target_id, version, mcu_version) if successful.
pub fn parse_version_info(data: &[u8]) -> Option<(u32, String, Option<String>)> {
    if data.len() < 5 {
        return None;
    }

    let target_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let mut i = 4;
    let ver_len = data[i] as usize;
    i += 1;

    if data.len() < i + ver_len + 1 {
        return None;
    }

    let version = std::str::from_utf8(&data[i..i + ver_len]).ok()?.to_string();
    i += ver_len;

    let flags_len = data[i] as usize;
    i += 1 + flags_len;

    let mcu_version = if data.len() > i {
        let mcu_len = data[i] as usize;
        i += 1;
        if data.len() >= i + mcu_len {
            let mcu = &data[i..i + mcu_len];
            let mcu = if !mcu.is_empty() && mcu[mcu.len() - 1] == 0 {
                &mcu[..mcu.len() - 1]
            } else {
                mcu
            };
            std::str::from_utf8(mcu).ok().map(|s| s.to_string())
        } else {
            None
        }
    } else {
        None
    };

    Some((target_id, version, mcu_version))
}

/// Parse installed apps from APDU response.
///
/// Returns a list of (app_name, app_hash) tuples.
pub fn parse_installed_apps(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut apps = Vec::new();

    if data.is_empty() || data[0] != 0x01 {
        return apps;
    }

    let mut i = 1;
    while i < data.len() {
        if data.len() < i + 1 + 2 + 2 + 32 + 32 + 1 {
            break;
        }

        let len = data[i] as usize;
        i += 1;
        i += 2; // blocks
        i += 2; // flags
        i += 32; // hash_code_data
        let hash = data[i..i + 32].to_vec();
        i += 32;
        let name_len = data[i] as usize;
        i += 1;

        if data.len() < i + name_len || len != name_len + 70 {
            break;
        }

        if let Ok(name) = std::str::from_utf8(&data[i..i + name_len]) {
            apps.push((name.to_string(), hash));
        }
        i += name_len;
    }

    apps
}
