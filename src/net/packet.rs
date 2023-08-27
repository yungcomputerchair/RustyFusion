#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]

#[repr(align(4))]
pub struct sP_CL2LS_REQ_LOGIN {
	szID: [u16; 33],
	szPassword: [u16; 33],
	iClientVerA: i32,
	iClientVerB: i32,
	iClientVerC: i32,
	iLoginType: i32,
	szCookie_TEGid: [u8; 64],
	szCookie_authid: [u8; 255]
}
