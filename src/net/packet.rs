#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]

#[repr(C)]
#[repr(align(4))]
pub struct sP_CL2LS_REQ_LOGIN {
	pub szID: [u16; 33],
	pub szPassword: [u16; 33],
	pub iClientVerA: i32,
	pub iClientVerB: i32,
	pub iClientVerC: i32,
	pub iLoginType: i32,
	pub szCookie_TEGid: [u8; 64],
	pub szCookie_authid: [u8; 255]
}
