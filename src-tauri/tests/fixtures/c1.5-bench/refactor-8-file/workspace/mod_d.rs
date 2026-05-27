// mod_d.rs — cross-file caller of old_name.
pub fn caller_d(x: i32) -> i32 {
    crate::mod_a::old_name(x) + 4
}

pub fn helper_d(x: i32) -> i32 {
    caller_d(x) * 4
}
