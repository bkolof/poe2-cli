//! Path of Building, running headless in an embedded LuaJIT.

mod code;
mod install;

use std::collections::BTreeMap;
use std::env;
use std::ffi::c_int;
use std::path::Path;

use anyhow::{Context, Result, bail};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value, ffi};
use serde::Deserialize;

pub use code::decode_build_code;
pub use install::{VERSION, ensure_installed, installed_dir};

unsafe extern "C-unwind" {
    fn luaopen_utf8(state: *mut ffi::lua_State) -> c_int;
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcResult {
    pub main_skill: Option<String>,
    pub stats: BTreeMap<String, serde_json::Value>,
}

impl CalcResult {
    pub fn number(&self, key: &str) -> Option<f64> {
        self.stats.get(key).and_then(serde_json::Value::as_f64)
    }
}

pub struct Pob {
    lua: Lua,
}

impl Pob {
    /// Boot PoB from an installed release. PoB resolves its files relative to
    /// its `src/` directory, so this changes the process's working directory.
    pub fn start(pob_dir: &Path) -> Result<Self> {
        let src = pob_dir.join("src");
        env::set_current_dir(&src).with_context(|| format!("cannot enter {}", src.display()))?;

        // PoB needs the full standard library (debug, jit, ffi), as under plain luajit.
        let lua = unsafe { Lua::unsafe_new() };
        let preload: Table = lua.globals().get::<Table>("package")?.get("preload")?;
        preload.set("lua-utf8", unsafe { lua.create_c_function(luaopen_utf8)? })?;
        lua.load(include_str!("boot.lua"))
            .set_name("boot.lua")
            .exec()?;

        let pob = Self { lua };

        if pob.lua.globals().get::<Value>("build")?.is_nil() {
            bail!("PoB failed to start:\n{}", pob.log_tail()?);
        }

        Ok(pob)
    }

    pub fn calculate(&self, build_xml: &str) -> Result<CalcResult> {
        let load: Function = self.lua.globals().get("loadBuildFromXML")?;
        load.call::<()>((build_xml, "poe2"))?;

        let result: Function = self.lua.load("poe2.result").eval()?;
        let value = result.call::<Value>(()).with_context(|| {
            format!(
                "PoB could not calculate the build:\n{}",
                self.log_tail().unwrap_or_default()
            )
        })?;

        Ok(self.lua.from_value(value)?)
    }

    fn log_tail(&self) -> Result<String> {
        let log: Vec<String> = self.lua.load("poe2.log").eval()?;
        let start = log.len().saturating_sub(20);
        Ok(log[start..].join("\n"))
    }
}
