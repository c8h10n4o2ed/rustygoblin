#![allow(unused_assignments, unused_variables, unused_mut, dead_code, unused_parens, deprecated, unused_imports)]

use std::fmt;
use std::collections::HashMap;
use std::sync::Mutex;

//This is not really documented anywhere except as a bug:
//https://github.com/rust-lang-nursery/lazy-static.rs/issues/120
use lazy_static::lazy_static;

lazy_static! {
    //Need a mutex here for mutable references later on in the code.
    //See: https://users.rust-lang.org/t/how-can-i-use-mutable-lazy-static/3751/2
    //See: https://stackoverflow.com/questions/34832583/global-mutable-hashmap-in-a-library
    static ref CORMAP: Mutex<HashMap<[u8; 16], u64>> = Mutex::new(HashMap::new());
}

//See: https://stackoverflow.com/questions/27650312/show-u8-slice-in-hex-representation
struct HexSlice<'a>(&'a [u8]);

impl<'a> HexSlice<'a> {
    fn new<T>(data: &'a T) -> HexSlice<'a> 
        where T: ?Sized + AsRef<[u8]> + 'a
    {
        HexSlice(data.as_ref())
    }
}

// You can even choose to implement multiple traits, like Lower and UpperHex
impl<'a> fmt::Display for HexSlice<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for byte in self.0 {
            // Decide if you want to pad out the value here
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl<'a> fmt::LowerHex for HexSlice<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for byte in self.0 {
            // Decide if you want to pad out the value here
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

pub fn calc_hash (packet: &[u8]) -> [u8; 16] {
    let digest = md5::compute(packet);
    return digest.0;
}

pub fn calc_hash_digest (packet: &[u8]) -> md5::Digest {
    let digest = md5::compute(packet);
    return digest;
}

pub fn add_packet_hash (digest: &[u8; 16]) {
    let mut mobj = CORMAP.lock().unwrap();
    let mut counter = mobj.entry(*digest).or_insert(0);
    *counter += 1;

    if (*counter > 1) {
        println!("hash({}) >= 2", HexSlice::new(digest));
    }
}

#[test]
fn test_hash_digest_1 () {
    let digest = calc_hash_digest(b"abcdefghijklmnopqrstuvwxyz");
    assert_eq!(format!("{:x}", digest), "c3fcd3d76192e4007dfb496cca67e13b");
}

#[test]
fn test_hash_digest_2 () {
    let digest = calc_hash(b"abcdefghijklmnopqrstuvwxyz");
    assert_eq!(format!("{:x}", HexSlice::new(&digest)), "c3fcd3d76192e4007dfb496cca67e13b");
}
