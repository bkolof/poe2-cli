fn main() {
    // PoB requires the lua-utf8 C module. Compile it into the binary against the
    // Lua 5.1 API headers of the LuaJIT that mlua vendors.
    cc::Build::new()
        .file("vendor/luautf8/lutf8lib.c")
        .include("vendor/luajit")
        .warnings(false)
        .compile("luautf8");
    println!("cargo:rerun-if-changed=vendor");
}
