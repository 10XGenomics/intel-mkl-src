// MIT License
//
// Copyright (c) 2017 Toshiki Teramura
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use anyhow::Result;
use intel_mkl_tool::*;
use ocipkg::{
    distribution::{get_layer_bytes, MediaType},
    ImageName,
};
use std::{env, fs, path::PathBuf, str::FromStr};

macro_rules! def_mkl_config {
    ($cfg:literal) => {
        #[cfg(feature = $cfg)]
        const MKL_CONFIG: &str = $cfg;
    };
}

def_mkl_config!("mkl-static-lp64-iomp");
def_mkl_config!("mkl-static-lp64-seq");
def_mkl_config!("mkl-static-ilp64-iomp");
def_mkl_config!("mkl-static-ilp64-seq");
def_mkl_config!("mkl-dynamic-lp64-iomp");
def_mkl_config!("mkl-dynamic-lp64-seq");
def_mkl_config!("mkl-dynamic-ilp64-iomp");
def_mkl_config!("mkl-dynamic-ilp64-seq");

// Default value
#[cfg(all(
    not(feature = "mkl-static-lp64-iomp"),
    not(feature = "mkl-static-lp64-seq"),
    not(feature = "mkl-static-ilp64-iomp"),
    not(feature = "mkl-static-ilp64-seq"),
    not(feature = "mkl-dynamic-lp64-iomp"),
    not(feature = "mkl-dynamic-lp64-seq"),
    not(feature = "mkl-dynamic-ilp64-iomp"),
    not(feature = "mkl-dynamic-ilp64-seq"),
))]
const MKL_CONFIG: &str = "mkl-static-ilp64-iomp";

fn main() -> Result<()> {
    let cfg = Config::from_str(MKL_CONFIG).unwrap();
    if let Ok(lib) = Library::new(cfg) {
        lib.print_cargo_metadata()?;
        return Ok(());
    }

    // Try ocipkg for static library.
    //
    // This does not work for dynamic library because the directory
    // where ocipkg download archive is not searched by ld
    // unless user set `LD_LIBRARY_PATH` explictly.
    if cfg.link == LinkType::Static {
        if cfg!(target_os = "linux") {
            link_package(&format!(
                "ghcr.io/rust-math/rust-mkl/linux/{}:2020.1-3038006115",
                MKL_CONFIG
            ))?;
        }
        if cfg!(target_os = "windows") {
            link_package(&format!(
                "ghcr.io/rust-math/rust-mkl/windows/{}:2022.0-3038006115",
                MKL_CONFIG
            ))?;
        }
    }

    Ok(())
}

/// Copied from ocipkg to effectively backport the addition made in this commit upstream:
/// https://github.com/termoshtt/ocipkg/commit/7b382892834e577ccf65dd7b368f796903722f63
fn link_package(image_name: &str) -> Result<()> {
    const STATIC_PREFIX: &str = if cfg!(target_os = "windows") {
        ""
    } else {
        "lib"
    };

    let image_name = ImageName::parse(image_name)?;

    let mut dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    if let Some(port) = image_name.port {
        dir.push(format!("{}__{}", image_name.hostname, port));
    } else {
        dir.push(&image_name.hostname);
    }
    dir.push(image_name.name.as_str());
    dir.push(format!("__{}", image_name.reference));

    if !dir.exists() {
        let blob = get_layer_bytes(&image_name, |media_type| {
            match media_type {
                MediaType::ImageLayerGzip => true,
                // application/vnd.docker.image.rootfs.diff.tar.gzip case
                MediaType::Other(ty) if ty.ends_with("tar.gzip") => true,
                _ => false,
            }
        })?;
        let buf = flate2::read::GzDecoder::new(blob.as_slice());

        log::info!("Get {} into {}", image_name, dir.display());
        tar::Archive::new(buf).unpack(&dir)?;
    }
    println!("cargo:rustc-link-search={}", dir.display());
    for path in fs::read_dir(&dir)?.filter_map(|entry| {
        let path = entry.ok()?.path();
        path.is_file().then(|| path)
    }) {
        let name = path
            .file_stem()
            .unwrap()
            .to_str()
            .expect("Non UTF-8 is not supported");
        let name = if let Some(name) = name.strip_prefix(STATIC_PREFIX) {
            name
        } else {
            continue;
        };
        if let Some(ext) = path.extension() {
            if ext == STATIC_EXTENSION {
                println!("cargo:rustc-link-lib=static={}", name);
            }
        }
    }
    println!("cargo:rerun-if-changed={}", dir.display());
    println!("cargo:rerun-if-env-changed=XDG_DATA_HOME");
    Ok(())
}
