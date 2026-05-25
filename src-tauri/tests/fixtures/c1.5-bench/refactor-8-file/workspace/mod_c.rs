// mod_c.rs — cross-file caller of old_name.
pub fn caller_c(x: i32) -> i32 {
    crate::mod_a::old_name(x) + 3
}

pub fn helper_c(x: i32) -> i32 {
    caller_c(x) * 3
}
