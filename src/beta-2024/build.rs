// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use serde_json::Value;
use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

const MANIFEST_JSON_URL: &str =
    "https://github.com/MystenLabs/sui/raw/mainnet/crates/sui-framework-snapshot/manifest.json";

// 定义与JSON结构匹配的数据模型
#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    path: String,
    id: String,
}

#[derive(Debug, Deserialize)]
struct VersionEntry {
    git_revision: String,
    packages: Vec<Package>,
}

/// 从远程拉取最新的system packages JSON并解析（假设按顺序排列，取最后一个）
fn fetch_latest_system_packages() -> anyhow::Result<Option<(u32, VersionEntry)>> {
    let response = ureq::get(MANIFEST_JSON_URL).call()?;
    let status = response.status();
    if !(200..=299).contains(&status) {
        return Err(anyhow::anyhow!(format!(
            "get json manifest.json: {}",
            response.status()
        )));
    }

    println!("start fetch json");
    let json_data: Value = serde_json::from_reader(response.into_reader())?;
    println!("{:?}", json_data);
    if let Value::Object(map) = json_data {
        let mut entries: Vec<(String, Value)> = map.into_iter().collect();
        if let Some((last_key, last_value)) = entries.pop() {
            if let Ok(version) = last_key.parse::<u32>() {
                let entry: VersionEntry = serde_json::from_value(last_value)?;
                return Ok(Some((version, entry)));
            }
        }
    }

    Ok(None)
}

fn generate_system_packages_version_table() -> anyhow::Result<()> {
    let (latest_version, latest_entry) = match fetch_latest_system_packages()? {
        Some(data) => data,
        None => return Err(anyhow::anyhow!("fetch_latest_system_packages failed.")),
    };

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("system_packages_version_table.rs");
    let mut file = BufWriter::new(File::create(&dest_path)?);

    writeln!(&mut file, "[")?;
    writeln!(
        &mut file,
        "  (ProtocolVersion::new( {latest_version:>2} ), SystemPackagesVersion {{"
    )?;
    writeln!(
        &mut file,
        "        git_revision: \"{}\".into(),",
        latest_entry.git_revision
    )?;
    writeln!(&mut file, "        packages: [")?;

    for package in latest_entry.packages.iter() {
        writeln!(
            &mut file,
            "          SystemPackage {{ package_name: \"{}\".into(), repo_path: \"{}\".into(), id: \"{}\".into() }},",
            package.name,
            package.path,
            package.id
        )?;
    }

    writeln!(&mut file, "        ].into(),")?;
    writeln!(&mut file, "      }}),")?;
    writeln!(&mut file, "]")?;

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo:rustc-env=SUI_SYS_PKG_TABLE={}", dest_path.display());
    Ok(())
}

fn main() {
    generate_system_packages_version_table().unwrap();
}
