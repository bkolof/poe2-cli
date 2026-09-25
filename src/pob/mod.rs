//! Path of Building, running headless in an embedded LuaJIT.

mod code;
mod install;
pub mod model;

use std::collections::BTreeMap;
use std::env;
use std::ffi::c_int;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use mlua::serde::SerializeOptions;
use mlua::{Function, IntoLuaMulti, Lua, LuaSerdeExt, Table, Value, ffi};
use serde::de::DeserializeOwned;

pub use code::{decode_build_code, encode_build_code};
pub use install::{VERSION, ensure_installed, installed_dir};
use model::{
    BuildInfo, GemInfo, Listing, ModInfo, SidebarRow, SkillDps, SlotUniques, SlotUpgrades,
    TradeQuery, TradeQueryRequest, TreeSuggestion, UniqueInfo, WhatIf, WhatIfRequest,
};

unsafe extern "C-unwind" {
    fn luaopen_utf8(state: *mut ffi::lua_State) -> c_int;
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

        pob.lua
            .load(include_str!("api.lua"))
            .set_name("api.lua")
            .exec()?;
        Ok(pob)
    }

    pub fn load(&self, build_xml: &str) -> Result<()> {
        self.call("load", build_xml)
    }

    pub fn build_site_url(&self, link: &str) -> Result<Option<String>> {
        self.call("buildSiteUrl", link)
    }

    pub fn info(&self) -> Result<BuildInfo> {
        self.call("info", ())
    }

    pub fn sidebar(&self) -> Result<Vec<SidebarRow>> {
        self.call("sidebar", ())
    }

    pub fn output(&self) -> Result<BTreeMap<String, serde_json::Value>> {
        self.call("output", ())
    }

    pub fn skills(&self) -> Result<Vec<SkillDps>> {
        self.call("skills", ())
    }

    pub fn what_if(&self, request: &WhatIfRequest) -> Result<WhatIf> {
        // Absent options must reach Lua as nil, which it treats as false.
        let options = SerializeOptions::new().serialize_none_to_null(false);
        let request = self.lua.to_value_with(request, options)?;
        self.call("whatIf", request)
    }

    pub fn tree_suggestions(&self, max_distance: u32) -> Result<Vec<TreeSuggestion>> {
        self.call("treeSuggestions", max_distance)
    }

    pub fn slot_upgrades(&self, slot: &str) -> Result<SlotUpgrades> {
        self.call("slotUpgrades", slot)
    }

    pub fn uniques_for_slot(&self, slot: &str) -> Result<SlotUniques> {
        self.call("uniquesForSlot", slot)
    }

    pub fn trade_query(&self, request: &TradeQueryRequest) -> Result<TradeQuery> {
        let request = self.lua.to_value(request)?;
        self.call("tradeQuery", request)
    }

    /// Calculate trade listings (fetch response bodies) in a slot.
    pub fn evaluate_listings(&self, slot: &str, bodies: &[String]) -> Result<Vec<Listing>> {
        self.call("evaluateListings", (slot, bodies.to_vec()))
    }

    pub fn search_mods(&self, query: &str, base: Option<&str>) -> Result<Vec<ModInfo>> {
        self.call("searchMods", (query, base))
    }

    pub fn search_gems(&self, query: &str) -> Result<Vec<GemInfo>> {
        self.call("searchGems", query)
    }

    pub fn search_uniques(&self, query: &str) -> Result<Vec<UniqueInfo>> {
        self.call("searchUniques", query)
    }

    /// Call `poe2.<name>` from `api.lua` and convert what it returns.
    fn call<R: DeserializeOwned>(&self, name: &str, args: impl IntoLuaMulti) -> Result<R> {
        let function: Function = self.lua.load(format!("poe2.{name}")).eval()?;
        let value = function.call::<Value>(args).map_err(lua_error)?;
        self.lua
            .from_value(value)
            .with_context(|| format!("unexpected result from PoB for {name}"))
    }

    fn log_tail(&self) -> Result<String> {
        let log: Vec<String> = self.lua.load("poe2.log").eval()?;
        let start = log.len().saturating_sub(20);
        Ok(log[start..].join("\n"))
    }
}

/// `api.lua` raises user-facing messages; keep those free of Lua tracebacks.
fn lua_error(error: mlua::Error) -> anyhow::Error {
    match error {
        mlua::Error::RuntimeError(message) => {
            anyhow!("{}", message.lines().next().unwrap_or_default())
        }
        mlua::Error::CallbackError { cause, .. } => lua_error((*cause).clone()),
        other => anyhow!(other),
    }
}
