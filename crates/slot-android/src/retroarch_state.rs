//! RetroArch RASTATE v1 container codec; based on libretro/RetroArch
//! tasks/task_save.c. It is intentionally strict about lengths and versions.
//!
//! This handles UNCOMPRESSED RASTATE data only. RetroArch's rzip files may
//! carry zlib/zstd compressed payloads: do not feed compressed bytes to a core
//! or overwrite an unreadable user state on export.
const MAX_CORE_STATE: usize = 64 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"RASTATE\x01";

fn block(out: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    while out.len() % 8 != 0 { out.push(0); }
}

pub fn encode(raw: &[u8]) -> Result<Vec<u8>, String> {
    if raw.is_empty() || raw.len() > MAX_CORE_STATE {
        return Err("Invalid libretro state length".into());
    }
    let mut output = Vec::with_capacity(raw.len() + 32);
    output.extend_from_slice(MAGIC);
    block(&mut output, b"MEM ", raw);
    block(&mut output, b"END ", &[]);
    Ok(output)
}

/// Extract the bare retro_serialize payload. If an unsupported compressed
/// format is encountered the caller MUST preserve the existing external file.
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.is_empty() || bytes.len() > MAX_CORE_STATE + 1024 * 1024 {
        return Err("Empty or oversized RetroArch state".into());
    }
    if bytes.starts_with(b"RZIP") || bytes.starts_with(b"RZ") {
        return Err("Compressed RetroArch state is not supported yet".into());
    }
    if bytes.len() >= 7 && &bytes[..7] == b"RASTATE" {
        if bytes.len() < 16 || &bytes[..8] != MAGIC {
            return Err("Unsupported RetroArch state version".into());
        }
        let mut pos=8;
        let mut memory: Option<Vec<u8>>=None;
        loop {
            if pos + 8 > bytes.len() {
                return Err("Truncated RetroArch block header".into());
            }
            let name=&bytes[pos..pos+4];
            let size=u32::from_le_bytes(bytes[pos+4..pos+8].try_into().unwrap()) as usize;
            pos+=8;
            if name==b"END " {
                if size!=0 { return Err("Invalid END block".into()); }
                return memory.ok_or_else(||"No libretro memory block".into());
            }
            let end=pos.checked_add(size).ok_or("State block overflow")?;
            if end > bytes.len() {return Err("RetroArch block exceeds file".into());}
            if name==b"MEM " {
                if memory.is_some() || size==0 || size>MAX_CORE_STATE {
                    return Err("Duplicate or invalid memory block".into());
                }
                memory=Some(bytes[pos..end].to_vec());
            }
            pos = end.checked_add(7).ok_or("Block align overflow")? & !7;
            if pos > bytes.len() {return Err("Truncated state block padding".into());}
        }
    }
    // RetroArch explicitly retains support for old raw libretro save states.
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retroarch_v1_round_trip_with_alignment() {
        for n in [1usize,7,8,9,256,16384] {
            let data=(0..n).map(|x|(x%251) as u8).collect::<Vec<_>>();
            let wrapped=encode(&data).unwrap();
            assert!(wrapped.starts_with(b"RASTATE\x01MEM "));
            assert!(wrapped.windows(4).any(|x|x==b"END "));
            assert_eq!(decode(&wrapped).unwrap(),data);
        }
    }
    #[test]
    fn truncated_unknown_corrupt_and_compressed_are_refused() {
        assert!(decode(b"RASTATE\x01").is_err());
        assert!(decode(b"RASTATE\x02MEM ").is_err());
        assert!(decode(b"RZIPNOT_SUPPORTED").is_err());
        assert!(decode(b"RASTATE\x01MEM \xff\xff\xff\x7f").is_err());
    }
    #[test]
    fn raw_legacy_states_are_accepted() {
        assert_eq!(decode(b"mGBA").unwrap(),b"mGBA");
    }
}
