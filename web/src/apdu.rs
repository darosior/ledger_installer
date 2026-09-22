//! APDU command definitions for WebHID communication.

use ledger_manager::ledger_apdu::APDUCommand;

/// APDU command to query device version information.
pub const GET_VERSION: APDUCommand<&[u8]> = APDUCommand {
    cla: 0xe0,
    ins: 0x01,
    p1: 0x00,
    p2: 0x00,
    data: &[],
};

/// APDU command to list installed applications.
pub const LIST_APPS: APDUCommand<&[u8]> = APDUCommand {
    cla: 0xe0,
    ins: 0xde,
    p1: 0x00,
    p2: 0x00,
    data: &[],
};

/// APDU command to continue listing applications (pagination).
pub const CONTINUE_LIST_APPS: APDUCommand<&[u8]> = APDUCommand {
    cla: 0xe0,
    ins: 0xdf,
    p1: 0x00,
    p2: 0x00,
    data: &[],
};

/// Deserialize an APDU command from a hex string.
pub fn deser_apdu_command(hex_str: &str) -> Result<APDUCommand<Vec<u8>>, String> {
    let bytes = hex::decode(hex_str).map_err(|e| e.to_string())?;
    if bytes.len() < 5 {
        return Err("Invalid command".into());
    }

    let (cla, ins, p1, p2, data_len) = (bytes[0], bytes[1], bytes[2], bytes[3], bytes[4] as usize);
    if bytes.len() != 5 + data_len {
        return Err("Invalid command".into());
    }

    Ok(APDUCommand {
        cla,
        ins,
        p1,
        p2,
        data: bytes[5..].to_vec(),
    })
}
