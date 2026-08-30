//! Trunk `post_build` hook. Reads the woff2 files listed in
//! [`op_fontpack::MANIFEST`], encodes them into a single content-hashed pack
//! in the Trunk staging directory, and injects
//! `<meta name="op-fonts" content="fonts-<hash>.pack">` into the staged
//! `index.html` so the wasm can find it with a page-relative fetch. Runs for
//! both Trunk targets, so each page carries its own copy of the pack.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn main() {
    let staging = std::env::var_os("TRUNK_STAGING_DIR")
        .map(PathBuf::from)
        .expect("TRUNK_STAGING_DIR is set by Trunk for hooks");
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../op-site/assets/fonts");

    let faces: Vec<op_fontpack::Face> = op_fontpack::MANIFEST
        .iter()
        .map(|entry| {
            let path = assets.join(entry.path);
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            assert!(bytes.starts_with(b"wOF2"), "{} is not woff2", entry.path);
            op_fontpack::Face {
                family: entry.family.to_owned(),
                weight: entry.weight.to_owned(),
                style: entry.style.to_owned(),
                metrics: entry
                    .metrics
                    .map(|(a, b, c)| (a.to_owned(), b.to_owned(), c.to_owned())),
                bytes,
            }
        })
        .collect();

    let pack = op_fontpack::encode(&faces);
    let hash = Sha256::digest(&pack);
    let name = format!(
        "fonts-{:x}.pack",
        u128::from_be_bytes(hash[..16].try_into().expect("16 bytes"))
    );
    std::fs::write(staging.join(&name), &pack).expect("write pack");

    let index = staging.join("index.html");
    let html = std::fs::read_to_string(&index).expect("read staged index.html");
    let meta = format!("<meta name=\"op-fonts\" content=\"{name}\" />");
    assert!(
        !html.contains("name=\"op-fonts\""),
        "op-fonts meta already present"
    );
    let html = html.replacen("</head>", &format!("{meta}</head>"), 1);
    assert!(
        html.contains("name=\"op-fonts\""),
        "no </head> in staged index.html"
    );
    std::fs::write(&index, html).expect("write staged index.html");

    println!(
        "op-assets: packed {} faces into {name} ({} bytes)",
        faces.len(),
        pack.len()
    );
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Every manifest entry must point at a real woff2 file in the assets.
    #[test]
    fn manifest_files_exist_and_are_woff2() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../op-site/assets/fonts");
        for entry in op_fontpack::MANIFEST {
            let path = assets.join(entry.path);
            let bytes =
                std::fs::read(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
            assert!(bytes.len() > 10_000, "{} looks truncated", entry.path);
            assert_eq!(&bytes[..4], b"wOF2", "{} is not woff2", entry.path);
        }
    }

    /// The pack must be byte-stable so its content hash is reproducible.
    #[test]
    fn encoding_is_deterministic() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../op-site/assets/fonts");
        let faces: Vec<op_fontpack::Face> = op_fontpack::MANIFEST
            .iter()
            .map(|e| op_fontpack::Face {
                family: e.family.to_owned(),
                weight: e.weight.to_owned(),
                style: e.style.to_owned(),
                metrics: e
                    .metrics
                    .map(|(a, b, c)| (a.to_owned(), b.to_owned(), c.to_owned())),
                bytes: std::fs::read(assets.join(e.path)).expect("read"),
            })
            .collect();
        assert_eq!(op_fontpack::encode(&faces), op_fontpack::encode(&faces));
        let decoded = op_fontpack::decode(&op_fontpack::encode(&faces)).expect("decode");
        assert_eq!(decoded.len(), op_fontpack::MANIFEST.len());
    }
}
