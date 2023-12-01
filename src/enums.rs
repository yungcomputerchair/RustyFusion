#![allow(non_camel_case_types)]

#[repr(i32)]
#[derive(PartialEq, FromPrimitive, ToPrimitive, Clone, Copy)]
pub enum eItemLocation {
    eIL_Equip,
    eIL_Inven,
    eIL_QInven,
    eIL_Bank,
    eIL__End,
}
