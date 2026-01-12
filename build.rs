#![allow(unused)]

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

struct HeadphoneResult {
    name: String,
    preamp: f32,
    ten_band_eq: [f32; 10],
}

fn url_decode(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap(),
                16,
            )
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let repo_url = "https://github.com/jaakkopasanen/AutoEq";
    let repo_dir = Path::new("/tmp/autoeq_repo");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("autoeq_db.rs");

    let mut output: std::process::Output;

    if !repo_dir.exists() {
        output = std::process::Command::new("git")
            .arg("clone")
            .arg("--depth=1")
            .arg(repo_url)
            .arg(repo_dir)
            .output()
            .expect("Failed to clone AutoEq repository");
    } else {
        output = std::process::Command::new("git")
            .current_dir(repo_dir)
            .arg("pull")
            .output()
            .expect("Failed to pull AutoEq repository");
    }

    if !output.status.success() {
        eprintln!("cargo:error=Failed to clone or update AutoEq repository");
        // We might not want to fail the build if net is down, but for now strict.
        std::process::exit(1);
    }

    let index_path = repo_dir.join("results/INDEX.md");
    if !index_path.exists() {
        eprintln!("cargo:warning=INDEX.md not found in AutoEq repo");
        return;
    }

    let index = std::fs::read_to_string(index_path).unwrap();
    let mut entries: HashMap<String, Vec<HeadphoneResult>> = HashMap::new();

    for line in index.lines() {
        if !line.starts_with("- [") {
            continue;
        }

        let parts: Vec<&str> = line.split("](").collect();
        if parts.len() < 2 {
            continue;
        }

        let name_part = parts[0].trim_start_matches("- [");
        let name = name_part;

        let link_part = parts[1];
        let mut end_index = 0;
        if let Some(idx) = link_part.find(')') {
            end_index = idx;
        }

        let result_link = &link_part[..end_index];
        let result_link = if let Some(stripped) = result_link.strip_prefix("./")
        {
            stripped
        } else {
            result_link
        };
        let result_link = url_decode(result_link);

        let fixed_band_path = repo_dir
            .join("results")
            .join(&result_link)
            .join(format!("{name} FixedBandEQ.txt", name = name));

        if !fixed_band_path.exists() {
            continue;
        }

        let content = match std::fs::read_to_string(&fixed_band_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut preamp = 0.0;
        let mut ten_band_eq = [0.0; 10];

        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            if parts[0] == "Preamp:" && parts.len() >= 2 {
                if let Ok(val) = parts[1].parse::<f32>() {
                    preamp = val;
                }
            } else if parts[0] == "Filter" && parts.len() >= 9 {
                let idx_str = parts[1].trim_end_matches(':');
                if let Ok(idx) = idx_str.parse::<usize>()
                    && idx >= 1
                    && let Ok(gain) = parts[8].parse::<f32>()
                {
                    ten_band_eq[idx - 1] = gain;
                }
            }
        }

        entries
            .entry(name.to_string())
            .or_default()
            .push(HeadphoneResult {
                name: name.to_string(),
                preamp,
                ten_band_eq,
            });
    }

    let mut file = BufWriter::new(
        File::create(&dest_path).expect("Failed to create output file"),
    );

    let mut map = phf_codegen::Map::new();

    let mut value_strings = Vec::new();
    for (name, results) in &entries {
        let mut results_str = String::new();
        results_str.push_str("&[");
        for (i, res) in results.iter().enumerate() {
            if i > 0 {
                results_str.push_str(", ");
            }
            results_str.push_str(&format!(
                "HeadphoneResult {{ name: {:?}, preamp: {:?}, ten_band_eq: {:?} }}",
                res.name, res.preamp, res.ten_band_eq
            ));
        }
        results_str.push(']');
        value_strings.push((name, results_str));
    }

    for (name, val_str) in &value_strings {
        map.entry(name, val_str);
    }

    writeln!(
        &mut file,
        "pub static AUTOEQ_DB: phf::Map<&'static str, &'static [HeadphoneResult]> = {};",
        map.build()
    )
    .unwrap();
}
