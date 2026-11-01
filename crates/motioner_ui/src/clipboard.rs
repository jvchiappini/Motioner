//! Copying text to the system clipboard.
//!
//! The engine exposes no clipboard API and the project forbids new
//! dependencies, so on Windows the "Copy" actions run PowerShell's
//! `Set-Clipboard`. Other platforms are a no-op (the UI still shows the text).

/// Copies `text` to the clipboard.
///
/// Returns `false` when the platform has no implementation or the call failed.
pub fn copy(text: &str) -> bool {
    #[cfg(windows)]
    {
        // `-EncodedCommand` takes UTF-16LE base64, so the text needs no shell
        // quoting at all (ids can contain quotes and dots).
        let script = format!("Set-Clipboard -Value {}", ps_single_quoted(text));
        let encoded = encode_utf16le_base64(&script);
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        false
    }
}

/// PowerShell single-quoted string: `'` becomes `''`.
#[cfg(windows)]
fn ps_single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Base64 of the UTF-16LE bytes of `script`, for `-EncodedCommand`.
#[cfg(windows)]
fn encode_utf16le_base64(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn powershell_quoting_escapes_single_quotes() {
        assert_eq!(ps_single_quoted("it's"), "'it''s'");
        assert_eq!(ps_single_quoted("plain"), "'plain'");
    }

    #[cfg(windows)]
    #[test]
    fn base64_matches_the_rfc_with_padding() {
        // "A" in UTF-16LE is 0x41 0x00 -> "QQA="
        assert_eq!(encode_utf16le_base64("A"), "QQA=");
        // "AB" is 0x41 0x00 0x42 0x00 -> "QQBCAA=="
        assert_eq!(encode_utf16le_base64("AB"), "QQBCAA==");
        assert_eq!(encode_utf16le_base64(""), "");
    }
}
