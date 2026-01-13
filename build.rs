#![allow(unused)]

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

struct HeadphoneResult {
    tester: String,
    variant: Option<String>,
    test_device: Option<String>,
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
        // example line:
        // - [1MORE Aero (ANC Off)](./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)) by HypetheSonics on GRAS RA0045
        // => transforms have been "compiled" in my head; might be wrong :)

        // all lines of interest start with "- ["
        if !line.starts_with("- [") {
            continue;
        }
        // =>
        // - [1MORE Aero (ANC Off)](./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)) by HypetheSonics on GRAS RA0045

        let parts: Vec<&str> = line.split("](").collect();
        // =>
        // ["- [1MORE Aero (ANC Off)", "./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)) by HypetheSonics on GRAS RA0045"]

        if parts.len() < 2 {
            continue;
        }

        let name_part = parts[0].trim_start_matches("- [");
        let name = name_part.to_string();
        // =>
        // 1MORE Aero (ANC Off)
        // println!("cargo:warning=name: {}", name);

        let variant: Option<String> = name_part
            .split(" (")
            .nth(1)
            .map(|s| s.trim_end_matches(")").to_string());
        // =>
        // Option("ANC Off")
        // println!("cargo:warning=variant: {:?}", variant);

        let link_part = parts[1];
        // =>
        // ./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)) by HypetheSonics on GRAS RA0045

        let Some(tester_part_str) = link_part.split(" by ").nth(1) else {
            continue;
        };
        // =>
        // HypetheSonics on GRAS RA0045

        let mut tester_parts = tester_part_str.split(" on ");
        // =>
        // Option(["HypetheSonics", "GRAS RA0045"])
        let tester_name = tester_parts.next().unwrap_or("").trim();
        if tester_name.is_empty() {
            continue;
        }
        // =>
        // HypetheSonics

        let test_device = tester_parts.next().map(|s| s.trim());
        // =>
        // GRAS RA0045

        // some fuckery required
        // because some links contain literal brackets as part of the link
        let mut end_index = 0;
        if let Some(idx) = link_part.rfind(')') {
            end_index = idx;
        }
        let result_link = &link_part[..end_index];
        // =>
        // ./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)

        let result_link = if let Some(stripped) = result_link.strip_prefix("./")
        {
            stripped
        } else {
            result_link
        };
        let result_link = url_decode(result_link);
        // =>
        // HypetheSonics/GRAS RA0045 in-ear/1MORE Aero (ANC Off)
        // println!("cargo:warning=result_link: {:?}", result_link);

        let fixed_band_path = repo_dir
            .join("results")
            .join(&result_link)
            .join(format!("{} FixedBandEQ.txt", name));

        // println!("cargo:warning=fixed_band_path: {:?}", fixed_band_path);

        if !fixed_band_path.exists() {
            continue;
        }
        // =>
        // /tmp/autoeq_repo/results/HypetheSonics/GRAS RA0045 in-ear/1MORE Aero (ANC Off) FixedBandEQ.txt

        // println!("cargo:warning=testy shmesty: {:?}", name);

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

        let base_name = name.split(" (").next().unwrap_or(&name);
        entries.entry(base_name.to_string()).or_default().push(
            HeadphoneResult {
                tester: tester_name.to_string(),
                variant,
                test_device: test_device.map(|s| s.to_string()),
                preamp,
                ten_band_eq,
            },
        );
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
                "HeadphoneResult {{ tester: {:?}, variant: {:?}, test_device: {:?}, preamp: {:?}, ten_band_eq: {:?} }}",
                res.tester, res.variant, res.test_device, res.preamp, res.ten_band_eq
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
