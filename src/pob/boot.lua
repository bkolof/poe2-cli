-- Boots PoB headless inside the poe2 binary. Rust sets the working directory
-- to PoB's src/ and preloads lua-utf8 before this runs.

-- The luajit executable sets `arg`, and PoB reads it on startup.
arg = {}
package.path = "../runtime/lua/?.lua;../runtime/lua/?/init.lua;" .. package.path

-- PoB logs through print(). Keep that out of the terminal, but hold on to it
-- so a failure can show what PoB said.
poe2 = { log = {} }
print = function(...)
	local parts = {}

	for i = 1, select("#", ...) do
		parts[i] = tostring(select(i, ...))
	end

	table.insert(poe2.log, table.concat(parts, "\t"))
end

-- On a startup error the wrapper waits for Enter. Never block on stdin.
io.read = function()
	return nil
end

dofile("HeadlessWrapper.lua")

