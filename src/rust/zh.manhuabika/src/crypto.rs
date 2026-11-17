use aidoku::{alloc::string::String, prelude::*};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn encrypt(data: &[u8], key: &[u8]) -> String {
	let mut mac = HmacSha256::new_from_slice(key).expect("");
	mac.update(data);
	let result = mac.finalize();
	let bytes = result.into_bytes();
	let mut signature = String::new();

	for byte in bytes.iter() {
		signature.push_str(&format!("{:02x}", byte));
	}

	signature
}
