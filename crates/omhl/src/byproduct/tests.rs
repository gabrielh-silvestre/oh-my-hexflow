use crate::byproduct::{ByproductErrorKind, ByproductHash, decode, encode};

#[test]
fn it_should_encode_the_header_followed_by_stdout_and_stderr() {
    let blob = encode(b"out\n", b"err\n");

    assert_eq!(blob, b"omhl-byproduct v1 stdout=4 stderr=4\nout\nerr\n");
}

#[test]
fn it_should_decode_what_it_encoded() {
    let cases: [(&[u8], &[u8]); 5] = [
        (b"out\n", b"err\n"),
        (b"", b""),
        (b"", b"only stderr\n"),
        (b"no trailing newline", b"nor here"),
        (b"line one\nline two\n", b""),
    ];

    for (stdout, stderr) in cases {
        let blob = encode(stdout, stderr);

        assert_eq!(decode(&blob), Some((stdout, stderr)));
    }
}

#[test]
fn it_should_refuse_a_blob_that_does_not_follow_the_format() {
    let blobs: [&[u8]; 5] = [
        b"",
        b"no header at all",
        b"omhl-byproduct v2 stdout=1 stderr=0\nx",
        b"omhl-byproduct v1 stdout=1 stderr=1\nx",
        b"omhl-byproduct v1 stdout=x stderr=0\n",
    ];

    let decoded: Vec<_> = blobs.iter().map(|blob| decode(blob)).collect();

    assert_eq!(decoded, vec![None; 5]);
}

// Blob store local por hash: `<db>/blobs/<2 primeiros hex>/<hash>`, gravação atômica e leitura conferida.

// NIST "abc"
const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[test]
fn it_should_reject_a_hash_that_is_not_64_lowercase_hex() {
    let invalid = [
        ABC_SHA256.to_uppercase(),
        ABC_SHA256[..63].to_owned(),
        ABC_SHA256.to_owned() + "0",
        "g".repeat(64),
        String::new(),
        "../".to_owned() + &"a".repeat(61),
    ];

    let kinds: Vec<_> = invalid
        .iter()
        .map(|text| ByproductHash::parse(text).err().map(|error| error.kind))
        .collect();
    let valid = ByproductHash::parse(ABC_SHA256);

    assert_eq!(kinds, vec![Some(ByproductErrorKind::InvalidHash); 6]);
    assert_eq!(
        valid.map(|parsed| parsed.as_str().to_owned()),
        Ok(ABC_SHA256.to_owned())
    );
}
