//! Generates the Telemetry Sigil Master Icon (.ico -> .res) and links it into the Windows .exe.

use std::env;
use std::fs;
use std::path::PathBuf;

fn blend(bg: (f32, f32, f32, f32), fg: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let a = fg.3 + bg.3 * (1.0 - fg.3);
    if a <= 0.0001 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let r = (fg.0 * fg.3 + bg.0 * bg.3 * (1.0 - fg.3)) / a;
    let g = (fg.1 * fg.3 + bg.1 * bg.3 * (1.0 - fg.3)) / a;
    let b = (fg.2 * fg.3 + bg.2 * bg.3 * (1.0 - fg.3)) / a;
    (r, g, b, a)
}

fn sd_rbox(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, rad: f32) -> f32 {
    let qx = (px - cx).abs() - (hx - rad);
    let qy = (py - cy).abs() - (hy - rad);
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - rad
}

/// Renders the Telemetry Sigil Master Icon at `size x size` in normalized [0..32] coordinates.
/// Returns bottom-up BGRA pixels + 1-bpp AND mask as a Win32 DIB (ICONIMAGE).
fn render_icon_dib(size: usize) -> Vec<u8> {
    let mut bgra = vec![0u8; size * size * 4];
    let scale = 32.0 / size as f32;
    let ss = if size <= 48 { 4 } else { 2 };
    let ss_f = ss as f32;

    for y in 0..size {
        for x in 0..size {
            let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let px = (x as f32 + (sx as f32 + 0.5) / ss_f) * scale;
                    let py = (y as f32 + (sy as f32 + 0.5) / ss_f) * scale;
                    let mut col = (0.0f32, 0.0f32, 0.0f32, 0.0f32);

                    // Master squircle backdrop (#05070D - #0D111A) with thin cyan/glass bevel
                    let d_sq = sd_rbox(px, py, 16.0, 16.0, 14.5, 14.5, 5.2);
                    if d_sq <= 0.0 {
                        let grad = ((px + py) / 64.0).clamp(0.0, 1.0);
                        col = (
                            5.0 + grad * 8.0,
                            7.0 + grad * 10.0,
                            13.0 + grad * 13.0,
                            1.0,
                        );
                        // Subtle inner bevel
                        if d_sq >= -0.85 {
                            col = blend(col, (0.0, 240.0, 255.0, 0.36));
                        }
                        // 4-corner HUD micro ticks
                        let in_corner_x = (3.4..=5.4).contains(&px) || (26.6..=28.6).contains(&px);
                        let in_corner_y = (3.4..=5.4).contains(&py) || (26.6..=28.6).contains(&py);
                        if in_corner_x && in_corner_y {
                            col = blend(col, (0.0, 240.0, 255.0, 0.25));
                        }
                    }

                    // 1. Rear monitor (Electric Cyan #00F0FF)
                    let d_rear = sd_rbox(px, py, 20.0, 12.2, 7.6, 5.6, 1.7);
                    if d_rear <= 0.0 && d_rear >= -1.8 {
                        col = blend(col, (0.0, 240.0, 255.0, 1.0));
                    }
                    if (19.6..=22.4).contains(&px) && (18.0..=19.8).contains(&py) {
                        col = blend(col, (0.0, 240.0, 255.0, 1.0));
                    }

                    // 2. Front monitor (Amber #FF9900)
                    let d_front = sd_rbox(px, py, 13.2, 17.6, 8.2, 5.8, 1.7);
                    if d_front <= 0.0 {
                        col = (10.0, 12.0, 18.0, 0.96);
                        if (7.2..=19.2).contains(&px) && (14.0..=21.2).contains(&py) {
                            let t = (px - 7.2) / 12.0;
                            let bar_top = 21.2 - t * 6.4;
                            if py >= bar_top {
                                let step = ((px - 7.2) / 3.0).floor();
                                let intensity = 0.48 + (3.0 - step).clamp(0.0, 3.0) * 0.17;
                                col = blend(col, (255.0, 153.0, 0.0, intensity));
                            } else {
                                col = blend(col, (255.0, 153.0, 0.0, 0.14));
                            }
                        }
                        if d_front >= -1.85 {
                            col = (255.0, 153.0, 0.0, 1.0);
                        }
                    }

                    // Front monitor stand
                    if ((11.7..=14.7).contains(&px) && (23.2..=25.4).contains(&py))
                        || ((9.0..=17.4).contains(&px) && (25.0..=27.0).contains(&py))
                    {
                        col = (255.0, 153.0, 0.0, 1.0);
                    }

                    acc.0 += col.0 * col.3;
                    acc.1 += col.1 * col.3;
                    acc.2 += col.2 * col.3;
                    acc.3 += col.3;
                }
            }
            let samples = (ss * ss) as f32;
            let a = (acc.3 / samples).clamp(0.0, 1.0);
            let dst_y = size - 1 - y; // DIB is bottom-up
            let idx = (dst_y * size + x) * 4;
            if a > 0.001 {
                bgra[idx] = (acc.2 / acc.3).clamp(0.0, 255.0) as u8;
                bgra[idx + 1] = (acc.1 / acc.3).clamp(0.0, 255.0) as u8;
                bgra[idx + 2] = (acc.0 / acc.3).clamp(0.0, 255.0) as u8;
                bgra[idx + 3] = (a * 255.0) as u8;
            }
        }
    }

    // BITMAPINFOHEADER (40 bytes) + XOR BGRA + 1-bpp AND mask (DWORD-aligned rows)
    let and_stride = ((size + 31) / 32) * 4;
    let and_size = and_stride * size;
    let mut dib = Vec::with_capacity(40 + bgra.len() + and_size);
    dib.extend_from_slice(&40u32.to_le_bytes()); // biSize
    dib.extend_from_slice(&(size as i32).to_le_bytes()); // biWidth
    dib.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // biHeight (XOR + AND)
    dib.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    dib.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    dib.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    dib.extend_from_slice(&((bgra.len() + and_size) as u32).to_le_bytes()); // biSizeImage
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&bgra);
    dib.resize(dib.len() + and_size, 0);
    dib
}

fn push_res_record(out: &mut Vec<u8>, res_type: u16, res_id: u16, data: &[u8]) {
    // Win32 .res resource header: DataSize(4), HeaderSize(4=32), Type(4), Name(4),
    // DataVersion(4), MemoryFlags(2), LanguageId(2), Version(4), Characteristics(4)
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&res_type.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&res_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&0x1030u16.to_le_bytes()); // MemoryFlags
    out.extend_from_slice(&0x0409u16.to_le_bytes()); // LanguageId (en-US)
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn build_res_bytes() -> Vec<u8> {
    let mut res = Vec::new();
    // Standard 32-byte empty .res header record
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&32u32.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());

    let sizes: [usize; 4] = [16, 32, 48, 256];
    let mut grp = Vec::new();
    grp.extend_from_slice(&0u16.to_le_bytes()); // idReserved
    grp.extend_from_slice(&1u16.to_le_bytes()); // idType = 1 (ICON)
    grp.extend_from_slice(&(sizes.len() as u16).to_le_bytes()); // idCount

    for (idx, &sz) in sizes.iter().enumerate() {
        let icon_id = (idx + 1) as u16;
        let dib = render_icon_dib(sz);
        push_res_record(&mut res, 3 /* RT_ICON */, icon_id, &dib);

        let dim_byte = if sz >= 256 { 0u8 } else { sz as u8 };
        grp.push(dim_byte); // bWidth
        grp.push(dim_byte); // bHeight
        grp.push(0); // bColorCount
        grp.push(0); // bReserved
        grp.extend_from_slice(&1u16.to_le_bytes()); // wPlanes
        grp.extend_from_slice(&32u16.to_le_bytes()); // wBitCount
        grp.extend_from_slice(&(dib.len() as u32).to_le_bytes()); // dwBytesInRes
        grp.extend_from_slice(&icon_id.to_le_bytes()); // nID
    }

    push_res_record(&mut res, 14 /* RT_GROUP_ICON */, 1, &grp);
    res
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
        let res_path = out_dir.join("app_icon.res");
        let bytes = build_res_bytes();
        fs::write(&res_path, bytes).expect("write app_icon.res");
        println!("cargo:rustc-link-arg-bins={}", res_path.display());
    }
}
