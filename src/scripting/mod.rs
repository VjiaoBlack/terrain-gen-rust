use mlua::prelude::*;

pub struct ScriptEngine {
    lua: Lua,
}

impl ScriptEngine {
    pub fn new() -> Result<Self, mlua::Error> {
        let lua = Lua::new();
        Ok(Self { lua })
    }

    /// Update game state variables accessible from Lua.
    pub fn update_state(
        &self,
        villager_count: u32,
        resources: &crate::ecs::Resources,
        season: &str,
        wolf_count: u32,
    ) -> Result<(), mlua::Error> {
        let globals = self.lua.globals();
        globals.set("villager_count", villager_count)?;
        globals.set("wolf_count", wolf_count)?;
        globals.set("season", season.to_string())?;

        let res_table = self.lua.create_table()?;
        res_table.set("food", resources.food)?;
        res_table.set("wood", resources.wood)?;
        res_table.set("stone", resources.stone)?;
        res_table.set("planks", resources.planks)?;
        res_table.set("masonry", resources.masonry)?;
        res_table.set("grain", resources.grain)?;
        res_table.set("bread", resources.bread)?;
        globals.set("resources", res_table)?;

        Ok(())
    }

    /// Load and execute a Lua script file.
    pub fn load_script(&self, path: &str) -> Result<(), mlua::Error> {
        let code =
            std::fs::read_to_string(path).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        self.lua.load(&code).exec()?;
        Ok(())
    }

    /// Execute a Lua code string directly.
    pub fn exec(&self, code: &str) -> Result<(), mlua::Error> {
        self.lua.load(code).exec()
    }

    /// Set a string global variable in Lua.
    pub fn set_global(&self, name: &str, value: &str) -> Result<(), mlua::Error> {
        self.lua.globals().set(name.to_string(), value.to_string())
    }

    /// Reload all .lua scripts from a directory (hot reload).
    /// This re-executes every .lua file, updating function definitions.
    pub fn reload_scripts(&self, dir: &str) -> Result<(), mlua::Error> {
        let entries =
            std::fs::read_dir(dir).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "lua") {
                self.load_script(&path.to_string_lossy())?;
            }
        }
        Ok(())
    }

    /// Call a Lua function by name, returning whether it exists.
    pub fn call_hook(&self, name: &str) -> Result<bool, mlua::Error> {
        let globals = self.lua.globals();
        match globals.get::<mlua::Function>(name) {
            Ok(func) => {
                func.call::<()>(())?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::Resources;

    #[test]
    fn test_engine_creates() {
        let engine = ScriptEngine::new().unwrap();
        // Verify Lua is functional
        engine.exec("x = 1 + 1").unwrap();
    }

    #[test]
    fn test_update_state_sets_globals() {
        let engine = ScriptEngine::new().unwrap();
        let res = Resources {
            food: 50,
            wood: 30,
            stone: 20,
            planks: 5,
            masonry: 3,
            grain: 10,
            bread: 8,
        };
        engine.update_state(12, &res, "Summer", 4).unwrap();

        // Read back from Lua
        engine
            .exec(
                r#"
            assert(villager_count == 12, "villager_count")
            assert(wolf_count == 4, "wolf_count")
            assert(season == "Summer", "season")
            assert(resources.food == 50, "food")
            assert(resources.wood == 30, "wood")
            assert(resources.stone == 20, "stone")
            assert(resources.planks == 5, "planks")
            assert(resources.masonry == 3, "masonry")
            assert(resources.grain == 10, "grain")
            assert(resources.bread == 8, "bread")
        "#,
            )
            .unwrap();
    }

    #[test]
    fn test_call_hook_missing() {
        let engine = ScriptEngine::new().unwrap();
        let called = engine.call_hook("nonexistent").unwrap();
        assert!(!called);
    }

    #[test]
    fn test_call_hook_exists() {
        let engine = ScriptEngine::new().unwrap();
        engine
            .exec("function on_tick() hook_called = true end")
            .unwrap();
        let called = engine.call_hook("on_tick").unwrap();
        assert!(called);
        engine.exec("assert(hook_called == true)").unwrap();
    }

    #[test]
    fn test_load_script() {
        let engine = ScriptEngine::new().unwrap();
        // Write a temp script
        let dir = std::env::temp_dir();
        let path = dir.join("test_script.lua");
        std::fs::write(&path, "test_var = 42").unwrap();
        engine.load_script(path.to_str().unwrap()).unwrap();
        engine.exec("assert(test_var == 42)").unwrap();
        std::fs::remove_file(&path).ok();
    }
}
