-- The functions the Rust side calls on a booted PoB. Each returns plain tables,
-- which mlua converts to Rust structs. Everything is computed by PoB itself:
-- stat comparisons go through the same calculator PoB uses for its item and
-- passive tooltips, and the stat lists are PoB's own sidebar definitions.

local function stripColors(text)
	return (text:gsub("%^x%x%x%x%x%x%x", ""):gsub("%^%d", ""))
end

local function recalculate()
	build.buildFlag = true
	runCallback("OnFrame")
end

local function formatStat(fmt, value, signed)
	return formatNumSep(string.format("%" .. (signed and "+" or "") .. fmt, value))
end

-- The stats that differ between two outputs, using PoB's display stat list
-- (the one behind its "Equipping this item will give you" tooltips).
local function compare(baseOutput, output)
	local changes = {}
	local env = build.calcsTab.mainEnv

	local function addChanges(statList, actor, before, after, prefix)
		for _, statData in ipairs(statList) do
			local shown = statData.stat and not statData.childStat and statData.stat ~= "SkillDPS"
				and (not statData.flag or actor.mainSkill.activeEffect.statSet.skillFlags[statData.flag])
			local old, new = before[statData.stat] or 0, after[statData.stat] or 0

			if shown and type(old) == "number" and type(new) == "number" then
				local diff = new - old
				local relevant = not statData.condFunc or statData.condFunc(new, after) or statData.condFunc(old, before)

				if math.abs(diff) > 0.001 and relevant then
					local scale = (statData.pc or statData.mod) and 100 or 1
					table.insert(changes, {
						stat = statData.stat,
						label = prefix .. statData.label,
						before = formatStat(statData.fmt, old * scale),
						after = formatStat(statData.fmt, new * scale),
						diff = formatStat(statData.fmt, diff * scale, true),
						percent = statData.compPercent and old ~= 0 and (new / old * 100 - 100) or nil,
						better = (statData.lowerIsBetter and diff < 0) or (not statData.lowerIsBetter and diff > 0),
					})
				end
			end
		end
	end

	if env.player.mainSkill.minion and baseOutput.Minion and output.Minion then
		addChanges(build.minionDisplayStats, env.minion, baseOutput.Minion, output.Minion, "Minion: ")
	end

	addChanges(build.displayStats, env.player, baseOutput, output, "")
	return changes
end

-- What a change breaks that PoB still counts, as the game would not: skills
-- reserving more Spirit than the build has, and items and gems whose
-- attribute requirements it no longer meets. Only new problems are listed.
local attributeNames = { Str = "Strength", Dex = "Dexterity", Int = "Intelligence" }

local function problems(baseOutput, output)
	local found = {}

	if (output.SpiritUnreserved or 0) < 0 and (baseOutput.SpiritUnreserved or 0) >= 0 then
		table.insert(found, string.format("reserves %d more Spirit than the build has", -output.SpiritUnreserved))
	end

	for _, attr in ipairs({ "Str", "Dex", "Int" }) do
		local function unmet(o)
			return (o["Req" .. attr] or 0) > (o[attr] or 0)
		end

		if unmet(output) and not unmet(baseOutput) then
			local source = output["Req" .. attr .. "Item"] or {}
			local name = source.sourceItem and source.sourceItem.name
				or source.sourceGem and source.sourceGem.nameSpec
			table.insert(found, string.format("%s %d %s, the build would have %d",
				name and (name .. " needs") or "the support gems need",
				output["Req" .. attr], attributeNames[attr], output[attr] or 0))
		end
	end

	-- An empty table would reach Rust as a map, not an empty list.
	return #found > 0 and found or nil
end

-- The headline numbers used to rank passives and mods against each other.
local function impact(baseOutput, output, points)
	local function total(o, stat)
		return (o[stat] or 0) + (o.Minion and o.Minion[stat] or 0)
	end

	local function percent(stat)
		local old = total(baseOutput, stat)
		return old ~= 0 and (total(output, stat) / old * 100 - 100) or 0
	end

	return {
		points = points,
		dpsPercent = percent("CombinedDPS"),
		ehpPercent = percent("TotalEHP"),
		life = (output.Life or 0) - (baseOutput.Life or 0),
		energyShield = (output.EnergyShield or 0) - (baseOutput.EnergyShield or 0),
		problems = problems(baseOutput, output),
	}
end

local function findSlot(name)
	for slotName, slot in pairs(build.itemsTab.slots) do
		if slotName:lower() == name:lower() then
			return slotName, slot
		end
	end

	error("no item slot named '" .. name .. "'", 0)
end

-- A passive by node id or name. Several passives can share a name, so a name
-- resolves to the allocated one (to unallocate) or the nearest reachable one.
-- Allocating needs a path from the tree, and ascendancy passives must belong
-- to the build's ascendancy, as in PoB itself.
local function findNode(name, allocated)
	local ascendancy = build.spec.curAscendClassName

	local function usable(node)
		if (node.alloc or false) ~= allocated then
			return false
		end

		if allocated then
			return true
		end

		return node.path ~= nil and (not node.ascendancyName or node.ascendancyName == ascendancy)
	end

	local byId = tonumber(name) and build.spec.nodes[tonumber(name)]
	local candidates = {}

	if byId then
		candidates = { byId }
	else
		for _, node in pairs(build.spec.nodes) do
			if node.dn and node.dn:lower() == name:lower() then
				table.insert(candidates, node)
			end
		end
	end

	if #candidates == 0 then
		error("no passive named '" .. name .. "'", 0)
	end

	local found

	for _, node in ipairs(candidates) do
		if usable(node) and (not found or (node.pathDist or math.huge) < (found.pathDist or math.huge)) then
			found = node
		end
	end

	if found then
		return found
	end

	local node = candidates[1]

	if allocated then
		error("'" .. node.dn .. "' is not allocated", 0)
	elseif node.alloc then
		error("'" .. node.dn .. "' is already allocated", 0)
	elseif node.ascendancyName and node.ascendancyName ~= ascendancy then
		error("'" .. node.dn .. "' belongs to the " .. node.ascendancyName .. " ascendancy", 0)
	end

	error("'" .. node.dn .. "' cannot be reached from the allocated passives", 0)
end

-- The raw-code download URL for a link to a build site PoB knows (pobb.in,
-- poe.ninja, Maxroll, ...), converted the way PoB's own import does it.
function poe2.buildSiteUrl(link)
	for _, site in ipairs(buildSites.websiteList) do
		if link:match(site.matchURL) then
			return (link:gsub(site.regexURL, site.downloadURL))
		end
	end

	return nil
end

-- PoB reports load errors through its GUI prompt, which also stops it from
-- processing further frames. Turn the prompt into an error and dismiss it.
function poe2.load(xml)
	launch.promptMsg = nil
	loadBuildFromXML(xml, "poe2")

	if launch.promptMsg then
		local prompt = stripColors(launch.promptMsg)
		launch.promptMsg = nil
		error("PoB could not load the build: " .. (prompt:match("Error:%s*\n\n(.-)\n") or prompt), 0)
	end
end

function poe2.info()
	local group = build.skillsTab.socketGroupList[build.mainSocketGroup]
	local ascendancy = build.spec.curAscendClassName

	return {
		class = build.spec.curClassName,
		ascendancy = ascendancy ~= "None" and ascendancy or nil,
		level = build.characterLevel,
		mainSkill = group and group.displayLabel,
	}
end

-- PoB's sidebar, as label/value rows. A row with neither is a section break.
function poe2.sidebar()
	build:RefreshStatList()
	local rows = {}

	for _, entry in ipairs(build.controls.statBox.list) do
		local label = entry[1] and stripColors(entry[1]):gsub(":$", "")
		local value = entry[2] and stripColors(entry[2])

		if label and label ~= "" then
			table.insert(rows, { label = label, value = value })
		elseif rows[#rows] and rows[#rows].label then
			table.insert(rows, {})
		end
	end

	return rows
end

-- Every number in PoB's main output. Non-finite numbers are dropped because
-- JSON cannot represent them.
function poe2.output()
	local stats = {}

	for key, value in pairs(build.calcsTab.mainOutput) do
		local kind = type(value)
		local finite = kind == "number" and value == value and math.abs(value) ~= math.huge

		if finite or kind == "string" or kind == "boolean" then
			stats[key] = value
		end
	end

	return stats
end

-- Calculates each active skill as if it were the main skill.
function poe2.skills()
	local groups = build.skillsTab.socketGroupList
	local mainGroup = build.mainSocketGroup
	local skills = {}

	for groupIndex, group in ipairs(groups) do
		local mainActive = group.mainActiveSkill

		for skillIndex, activeSkill in ipairs(group.displaySkillList or {}) do
			build.mainSocketGroup = groupIndex
			group.mainActiveSkill = skillIndex
			recalculate()
			local output = build.calcsTab.mainOutput

			local source = group.source

			table.insert(skills, {
				name = activeSkill.activeEffect.grantedEffect.name,
				group = group.displayLabel,
				slot = group.slot,
				grantedBy = source and ((source:match("^Tree") and "passive tree") or (source:match("^Item") and "item") or source:lower()),
				main = groupIndex == mainGroup and skillIndex == (mainActive or 1),
				-- PoB rates some skills (cooldowns, combos) by damage per use;
				-- their CombinedDPS is then that damage, not a rate.
				perUse = activeSkill.skillData.showAverage and true or false,
				combinedDps = output.CombinedDPS or 0,
				hitDps = output.TotalDPS or 0,
				dotDps = output.TotalDotDPS or 0,
				minionDps = output.Minion and output.Minion.CombinedDPS or 0,
				averageDamage = output.AverageHit or output.AverageDamage or 0,
				speed = output.Speed or 0,
			})
		end

		group.mainActiveSkill = mainActive
	end

	build.mainSocketGroup = mainGroup
	recalculate()
	return skills
end

-- Stat changes from equipping an item (in every slot it fits, or one slot)
-- and/or allocating and unallocating passives. Allocating includes the path
-- to the node; unallocating includes the nodes that depend on it.
function poe2.whatIf(request)
	local override = {}
	local points = { added = 0, removed = 0 }
	local passives = {}

	local function describe(node, action, count)
		table.insert(passives, {
			id = node.id,
			name = node.dn,
			action = action,
			ascendancy = node.ascendancyName,
			points = count,
		})
	end

	if request.allocate and #request.allocate > 0 then
		override.addNodes = {}

		for _, name in ipairs(request.allocate) do
			local node = findNode(name, false)
			local count = 0

			-- A path can run through passives that are already allocated.
			for _, pathNode in ipairs(node.path) do
				if not pathNode.alloc and not override.addNodes[pathNode] then
					override.addNodes[pathNode] = true
					count = count + 1
				end
			end

			points.added = points.added + count
			describe(node, "allocate", count)
		end
	end

	if request.unallocate and #request.unallocate > 0 then
		override.removeNodes = {}

		for _, name in ipairs(request.unallocate) do
			local node = findNode(name, true)
			local count = 0

			for _, dependent in ipairs(node.depends or { node }) do
				if not override.removeNodes[dependent] then
					override.removeNodes[dependent] = true
					count = count + 1
				end
			end

			points.removed = points.removed + count
			describe(node, "unallocate", count)
		end
	end

	local calcFunc, baseOutput = build.calcsTab:GetMiscCalculator()

	if not request.item then
		local output = calcFunc(override)
		return { points = points, passives = passives, results = { { changes = compare(baseOutput, output), problems = problems(baseOutput, output) } } }
	end

	local item = new("Item", request.item)

	if not item.base then
		error("PoB does not recognise this item; paste the full text copied in game with Ctrl+C", 0)
	end

	local itemsTab = build.itemsTab
	local itemSet = itemsTab.activeItemSet
	itemsTab:UpdateSockets()

	-- A jewel socket holds a jewel only when it is, or is being, allocated.
	if request.slot then
		local name, slot = findSlot(request.slot)
		local allocating = override.addNodes and override.addNodes[build.spec.nodes[slot.nodeId]]

		if slot.nodeId and slot.inactive and not allocating then
			error(name .. " is not allocated; add --allocate " .. slot.nodeId, 0)
		end
	end

	local activeSet = itemSet.useSecondWeaponSet and 2 or 1
	local slots = { [1] = {}, [2] = {} }
	local modDB = build.calcsTab.mainEnv.modDB
	-- As PoB's calculation counts them; it only reports the count in breakdowns.
	local charmLimit = math.min(modDB:Override(nil, "CharmLimit") or modDB:Sum("BASE", nil, "CharmLimit"), 3)

	for slotName, slot in pairs(itemsTab.slots) do
		local wanted = not request.slot or slotName:lower() == request.slot:lower()
		-- A jewel socket being allocated in the same comparison can take a jewel.
		local allocating = slot.nodeId and override.addNodes and override.addNodes[build.spec.nodes[slot.nodeId]]
		-- Weapon slots are shown for the active set only; both sets are compared.
		local active = allocating or not slot.inactive and (slot.weaponSet ~= nil or slot.shown())
		-- PoB's calculation ignores charms beyond the belt's charm slots.
		local charm = tonumber(slotName:match("^Charm (%d)$"))
		local usable = not charm or charm <= charmLimit

		if wanted and active and usable and itemsTab:IsItemValidForSlot(item, slotName) then
			table.insert(slots[slot.weaponSet or activeSet], slotName)
		end
	end

	local results = {}

	local function compareIn(slotNames, calc, base)
		for _, slotName in ipairs(slotNames) do
			local current = itemsTab.items[itemsTab.slots[slotName].selItemId]
			override.repSlotName = slotName
			override.repItem = item
			local output = calc(override)

			table.insert(results, {
				slot = slotName,
				replacing = current and current.name,
				changes = compare(base, output),
				problems = problems(base, output),
			})
		end
	end

	compareIn(slots[activeSet], calcFunc, baseOutput)
	local otherSet = 3 - activeSet

	-- The other weapon set is compared with it active, as when swapping to it:
	-- when asked for, or when it has a weapon.
	local otherWeapon = itemsTab.slots[otherSet == 2 and "Weapon 1 Swap" or "Weapon 1"].selItemId or 0
	local useOther = request.slot or otherWeapon ~= 0

	if useOther and #slots[otherSet] > 0 then
		itemSet.useSecondWeaponSet = otherSet == 2
		recalculate()
		local ok, err = pcall(function()
			compareIn(slots[otherSet], build.calcsTab:GetMiscCalculator())
		end)
		itemSet.useSecondWeaponSet = activeSet == 2
		recalculate()

		if not ok then
			error(err, 0)
		end
	end

	if #results == 0 then
		error("the item fits no usable slot" .. (request.slot and (" named '" .. request.slot .. "'") or ""), 0)
	end

	table.sort(results, function(a, b) return a.slot < b.slot end)
	return { item = item.name, points = points, passives = passives, results = results }
end

-- The impact of every unallocated passive within reach, including the path
-- to it. Ascendancy nodes are left out; they come from a separate budget.
function poe2.treeSuggestions(maxDistance)
	local calcFunc, baseOutput = build.calcsTab:GetMiscCalculator()
	local granted = build.calcsTab.mainEnv.grantedPassives
	local suggestions = {}

	for nodeId, node in pairs(build.spec.nodes) do
		local reachable = node.path and node.pathDist and node.pathDist <= maxDistance

		if not node.alloc and reachable and node.modKey ~= "" and not granted[nodeId] and not node.ascendancyName then
			local path = {}

			for _, pathNode in ipairs(node.path) do
				path[pathNode] = true
			end

			local result = impact(baseOutput, calcFunc({ addNodes = path }), node.pathDist)
			result.id = nodeId
			result.name = node.dn
			result.kind = node.type
			result.stats = node.sd
			table.insert(suggestions, result)
		end
	end

	return suggestions
end

-- A mod line with its numbers and value ranges replaced by #, so rolled item
-- text and the mod database's range text can be compared.
local function modTemplate(line)
	return (line:gsub("%(%-?[%d%.]+%-%-?[%d%.]+%)", "#"):gsub("[%d%.]+", "#"))
end

-- The impact of adding each mod the item in a slot could roll, one at a time,
-- at the middle of its value range. Uses the best tier the item level allows.
-- Imported items only carry mod text, so mods the item already has are
-- recognised by their text with the numbers ignored.
function poe2.slotUpgrades(slotName)
	local name, slot = findSlot(slotName)
	local item = build.itemsTab.items[slot.selItemId]

	if not item then
		error("nothing is equipped in " .. name, 0)
	end

	if item.rarity ~= "MAGIC" and item.rarity ~= "RARE" then
		error(item.name .. " in " .. name .. " is " .. item.rarity:lower() .. "; upgrades works on magic and rare items", 0)
	end

	-- The mod pool and affix limits PoB uses for this kind of item.
	local modPools = { Jewel = data.itemMods.Jewel, Flask = data.itemMods.Flask, Charm = data.itemMods.Charm }
	local modPool = modPools[item.type] or data.itemMods.Item
	local limit = item.rarity == "MAGIC" and 1 or (item.type == "Jewel" and 2 or 3)

	local itemLevel = item.itemLevel or 100
	local existing = {}

	for _, modLine in ipairs(item.explicitModLines) do
		existing[modTemplate(modLine.line)] = true
	end

	-- A mod is on the item when all its lines are; matching one line of a
	-- hybrid mod is not enough.
	local function onItem(mod)
		for _, line in ipairs(mod) do
			if not existing[modTemplate(line)] then
				return false
			end
		end

		return true
	end

	-- The groups the item has (a hybrid mod counts once), then the best tier of
	-- each group it could gain. An item cannot have two mods of one group.
	local rollable = {}
	local used = { Prefix = {}, Suffix = {} }

	for modId, mod in pairs(modPool) do
		local affix = mod.type == "Prefix" or mod.type == "Suffix"

		if affix and item:GetModSpawnWeight(mod) > 0 then
			rollable[modId] = mod

			if onItem(mod) then
				used[mod.type][mod.group] = true
			end
		end
	end

	local bestTier = {}

	for modId, mod in pairs(rollable) do
		local absent = not used.Prefix[mod.group] and not used.Suffix[mod.group]

		if absent and (mod.level or 0) <= itemLevel then
			local best = bestTier[mod.group]

			if not best or mod.level > best.level then
				bestTier[mod.group] = { id = modId, mod = mod, level = mod.level }
			end
		end
	end

	local free = {}

	for affix, groups in pairs(used) do
		local count = 0

		for _ in pairs(groups) do
			count = count + 1
		end

		free[affix] = math.max(0, limit - count)
	end

	local calcFunc, baseOutput = build.calcsTab:GetMiscCalculator()
	local raw = item:BuildRaw()
	local upgrades = {}

	for _, tier in pairs(bestTier) do
		local testItem = new("Item", raw)
		local lines = {}

		for _, line in ipairs(tier.mod) do
			local rolled = itemLib.applyRange(line, 0.5)
			table.insert(lines, rolled)
			table.insert(testItem.explicitModLines, { line = rolled, custom = true })
		end

		testItem:BuildAndParseRaw()
		local result = impact(baseOutput, calcFunc({ repSlotName = name, repItem = testItem }), 1)
		result.mod = table.concat(lines, " / ")
		result.affix = tier.mod.type
		result.level = tier.level
		result.fits = free[tier.mod.type] > 0
		table.insert(upgrades, result)
	end

	return {
		slot = name,
		item = item.name,
		itemLevel = itemLevel,
		corrupted = item.corrupted or false,
		freePrefixes = free.Prefix,
		freeSuffixes = free.Suffix,
		upgrades = upgrades,
	}
end

-- The impact of equipping each unique that fits a slot, in its current
-- version and with middle rolls, in place of what the slot holds now.
function poe2.uniquesForSlot(slotName)
	local name = findSlot(slotName)
	local calcFunc, baseOutput = build.calcsTab:GetMiscCalculator()
	local candidates = {}

	for _, list in pairs(data.uniques) do
		for _, raw in ipairs(list) do
			local item = new("Item", raw)

			if item.base and build.itemsTab:IsItemValidForSlot(item, name) then
				local result = impact(baseOutput, calcFunc({ repSlotName = name, repItem = item }), 1)
				result.name = item.title
				result.base = item.baseName
				result.levelRequired = item.requirements.level or 0
				table.insert(candidates, result)
			end
		end
	end

	return { slot = name, candidates = candidates }
end

local function contains(text, query)
	return text and text:lower():find(query:lower(), 1, true) ~= nil
end

-- Item mods whose text matches, optionally only those that can roll on a base.
function poe2.searchMods(query, baseName)
	local testItem

	if baseName then
		testItem = new("Item", "Rarity: RARE\nTest\n" .. baseName)

		if not testItem.base then
			error("no item base named '" .. baseName .. "'", 0)
		end
	end

	local mods = {}

	for kind, list in pairs(data.itemMods) do
		for modId, mod in pairs(list) do
			local matches = type(mod) == "table" and type(mod[1]) == "string" and (contains(mod[1], query) or contains(mod[2], query))

			if matches and (not testItem or (mod.weightKey and testItem:GetModSpawnWeight(mod) > 0)) then
				local lines = {}

				for _, line in ipairs(mod) do
					table.insert(lines, line)
				end

				local tags = {}

				for i, key in ipairs(mod.weightKey or {}) do
					if (mod.weightVal[i] or 0) > 0 then
						table.insert(tags, key)
					end
				end

				table.insert(mods, {
					id = modId,
					kind = kind,
					affix = mod.type,
					name = mod.affix,
					group = mod.group,
					level = mod.level,
					lines = lines,
					tags = tags,
				})
			end
		end
	end

	return mods
end

function poe2.searchGems(query)
	local gems = {}

	for _, gem in pairs(data.gems) do
		if contains(gem.name, query) or contains(gem.tagString, query) then
			local effect = gem.grantedEffect or {}

			table.insert(gems, {
				name = gem.name,
				kind = gem.gemType,
				tags = gem.tagString,
				support = effect.support or false,
				description = effect.description,
				requirements = { str = gem.reqStr or 0, dex = gem.reqDex or 0, int = gem.reqInt or 0 },
				maxLevel = gem.naturalMaxLevel,
			})
		end
	end

	return gems
end

-- A unique's text as the current game version has it: PoB keeps the lines of
-- older versions behind {variant:n} markers, and the last variant is current.
local function currentVariantText(raw)
	local variants = 0

	for _ in raw:gmatch("\nVariant: ") do
		variants = variants + 1
	end

	local lines = {}

	for line in raw:gmatch("[^\n]+") do
		local variantList = line:match("{variant:([%d,]+)}")
		local current = not variantList or variants == 0

		if variantList and not current then
			for variant in variantList:gmatch("%d+") do
				current = current or tonumber(variant) == variants
			end
		end

		if current and not line:match("^Variant: ") and not line:match("^Selected Variant: ") then
			table.insert(lines, (line:gsub("{[^}]*}", "")))
		end
	end

	return table.concat(lines, "\n")
end

function poe2.searchUniques(query)
	local uniques = {}

	for kind, list in pairs(data.uniques) do
		for _, raw in ipairs(list) do
			if contains(raw, query) then
				table.insert(uniques, { name = raw:match("^[^\n]+"), kind = kind, text = currentVariantText(raw) })
			end
		end
	end

	return uniques
end

-- Trade searches -------------------------------------------------------------

local tradeHelpers = LoadModule("Classes/TradeHelpers")

-- What each ranking weighs, in PoB's trade weight terms. Balanced is PoB's
-- own default.
local tradeStatWeights = {
	balanced = { { stat = "FullDPS", weightMult = 1 }, { stat = "TotalEHP", weightMult = 0.5 } },
	dps = { { stat = "FullDPS", weightMult = 1 } },
	ehp = { { stat = "TotalEHP", weightMult = 1 } },
}

-- The listing statuses PoB's generator knows, by its index.
local tradeStatuses = { securable = 1, available = 2, onlineleague = 3, online = 4, any = 5 }

-- Trade site stats matching a text or id, best first: an exact id, then pseudo
-- totals, explicit, implicit and other mods; within those, the shortest text
-- is the most specific.
local tradeStatRank = { pseudo = 0, explicit = 1, implicit = 2 }

function poe2.tradeStats(query)
	local matches = {}

	for _, category in ipairs(tradeHelpers.getTradeStats()) do
		for _, entry in ipairs(category.entries) do
			if entry.id == query then
				return { { id = entry.id, text = entry.text, kind = category.id } }
			end

			if contains(entry.text, query) then
				table.insert(matches, {
					id = entry.id,
					text = entry.text,
					kind = category.id,
					rank = tradeStatRank[category.id] or 3,
				})
			end
		end
	end

	table.sort(matches, function(a, b)
		if a.rank ~= b.rank then
			return a.rank < b.rank
		end

		if #a.text ~= #b.text then
			return #a.text < #b.text
		end

		return a.id < b.id
	end)

	for _, match in ipairs(matches) do
		match.rank = nil
	end

	return matches
end

local function tradeStatText(id)
	for _, category in ipairs(tradeHelpers.getTradeStats()) do
		for _, entry in ipairs(category.entries) do
			if entry.id == id then
				return entry.text
			end
		end
	end
end

-- The slot a trade search calculates in. `jewel` picks an allocated jewel
-- socket that has no jewel; a socket holds a jewel only when allocated.
function poe2.tradeSlot(slotName)
	local sockets = {}

	for nodeId, socket in pairs(build.itemsTab.sockets) do
		if build.spec.allocNodes[nodeId] then
			table.insert(sockets, socket)
		end
	end

	table.sort(sockets, function(a, b)
		return a.nodeId < b.nodeId
	end)

	if slotName:lower() == "jewel" then
		local filled = {}

		for _, socket in ipairs(sockets) do
			local jewel = build.itemsTab.items[socket.selItemId]

			if not jewel then
				return socket.slotName
			end

			table.insert(filled, socket.slotName .. " (" .. jewel.name .. ")")
		end

		if #filled == 0 then
			error("the build has no allocated jewel socket", 0)
		end

		error("every allocated jewel socket has a jewel; name one: " .. table.concat(filled, ", "), 0)
	end

	local name, slot = findSlot(slotName)

	if slot.nodeId and not build.spec.allocNodes[slot.nodeId] then
		error(name .. " is not allocated", 0)
	end

	return name
end

-- A unique by its exact name, with its base, for a trade search by name.
function poe2.findUnique(query)
	for _, list in pairs(data.uniques) do
		for _, raw in ipairs(list) do
			if raw:match("^[^\n]+"):lower() == query:lower() then
				local item = new("Item", raw)
				return { name = item.title, base = item.baseName }
			end
		end
	end

	error("no unique named '" .. query .. "'", 0)
end

-- A weighted trade search for a slot, generated by PoB's own trade query
-- generator: each stat's weight is what PoB calculates it is worth to the
-- build, most important first. Prices are capped in exalted orb equivalents.
-- The Rust side adds the user's own stat groups, filters and sort.
function poe2.tradeQuery(request)
	local name, slot = findSlot(request.slot)

	-- Without an item, PoB cannot tell which item category to search. An
	-- empty jewel socket is searched for jewels of the requested type.
	if not slot.nodeId and (slot.selItemId or 0) == 0 then
		error("nothing is equipped in " .. name, 0)
	end

	local status = tradeStatuses[request.status]
	local weights = tradeStatWeights[request.by]

	if not status or not weights then
		error("unknown status or ranking", 0)
	end

	local generator = new("TradeQueryGenerator", { itemsTab = build.itemsTab })
	local generated = {}
	generator.requesterCallback = function(_, json, err)
		generated.json, generated.err = json, err
	end
	generator.requesterContext = {}
	generator.tradeTypeIndex = status
	generator:StartQuery(slot, {
		statWeights = weights,
		maxPrice = request.maxExalted,
		maxLevel = build.characterLevel,
		includeCorrupted = true,
		includeMirrored = false,
		jewelType = request.jewelType,
	})

	-- The generator is a coroutine PoB steps once per frame.
	while generator.calcContext.co do
		generator:OnFrame()
	end

	if generated.err then
		error(generated.err, 0)
	end

	if not generated.json then
		error("PoB cannot generate a trade search for " .. name, 0)
	end

	local weightList = {}

	for _, weight in ipairs(generator.modWeights) do
		table.insert(weightList, { id = weight.tradeModId, text = tradeStatText(weight.tradeModId), weight = weight.weight })
	end

	return { slot = name, query = generated.json, weights = weightList }
end

-- Price checks ----------------------------------------------------------------

local buySimilar = LoadModule("Classes/CompareBuySimilar")

-- The slot whose trade category an item is searched in.
local function priceSlot(item)
	local itemType = item.type

	if itemType == "Ring" then
		return "Ring 1"
	elseif itemType == "Jewel" then
		return item.base.subType == "Charm" and "Charm 1" or "Jewel"
	elseif itemType == "Charm" then
		return "Charm 1"
	elseif itemType == "Flask" then
		return item.base.subType == "Mana" and "Flask 2" or "Flask 1"
	end

	for _, slotName in ipairs({ "Body Armour", "Helmet", "Gloves", "Boots", "Amulet", "Belt" }) do
		if itemType == slotName then
			return slotName
		end
	end

	return "Weapon 1"
end

-- A mod line with every number and range replaced by #, and its ranges in
-- order (a fixed number is a range of one value).
local function rangeTemplate(line)
	local ranges, i = {}, 1

	while true do
		local rangeStart, rangeEnd, low, high = line:find("%((%d+%.?%d*)%-(%d+%.?%d*)%)", i)
		local numberStart, numberEnd, number = line:find("(%d+%.?%d*)", i)

		if not numberStart then
			break
		end

		if rangeStart and rangeStart < numberStart then
			table.insert(ranges, { tonumber(low), tonumber(high) })
			i = rangeEnd + 1
		else
			table.insert(ranges, { tonumber(number), tonumber(number) })
			i = numberEnd + 1
		end
	end

	local template = line:gsub("%((%d+%.?%d*)%-(%d+%.?%d*)%)", "#"):gsub("%d+%.?%d*", "#")
	return template, ranges
end

-- How high a line rolled within a mod line's ranges (0 to 1), or nil when it
-- is not that mod line.
local function rollWithin(itemLine, modLine)
	local itemTemplate, values = rangeTemplate(itemLine)
	local modTemplate, ranges = rangeTemplate(modLine)

	if itemTemplate ~= modTemplate or #values ~= #ranges then
		return nil
	end

	local roll = 0.5

	for index, range in ipairs(ranges) do
		local low, high = math.min(range[1], range[2]), math.max(range[1], range[2])
		local value = values[index][1]

		if value < low or value > high then
			return nil
		end

		if high > low then
			roll = (value - low) / (high - low)
		end
	end

	return roll
end

-- A mod line with its ranges rolled at `roll` (0 to 1).
local function rollLine(line, roll)
	return (line:gsub("%((%d+%.?%d*)%-(%d+%.?%d*)%)", function(low, high)
		local value = tonumber(low) + roll * (tonumber(high) - tonumber(low))
		local whole = not low:find("%.") and not high:find("%.")
		return whole and tostring(math.floor(value + 0.5)) or string.format("%.1f", value)
	end))
end

-- The Martial Artist's Fists of Stone turns equipped gloves into Fists of
-- Stone and each explicit mod into a HandWraps version of it. The gloves are
-- listed as they were, so each HandWraps mod is matched by its lines and
-- values, and replaced by the mod it came from (HandWrapsFireResist4 from
-- FireResist4), rolled as high within its range. Several mods can become the
-- same HandWraps lines; those cannot be traced back.
local function untransformFistsOfStone(item)
	local lines = {}

	for _, list in ipairs({ item.implicitModLines, item.explicitModLines }) do
		for _, modLine in ipairs(list) do
			table.insert(lines, modLine.line)
		end
	end

	local matches = {}

	for modId, mod in pairs(data.itemMods.Item) do
		if modId:match("^HandWraps") then
			local used, roll = {}, nil

			for _, modLine in ipairs(mod) do
				local found

				for index, line in ipairs(lines) do
					local lineRoll = not used[index] and rollWithin(line, modLine)

					if lineRoll then
						found = index
						roll = roll or lineRoll
						break
					end
				end

				if not found then
					used = nil
					break
				end

				used[found] = true
			end

			if used then
				local indices = {}

				for index in pairs(used) do
					table.insert(indices, index)
				end

				table.sort(indices)
				local key = table.concat(indices, ",")
				matches[key] = matches[key] or { indices = indices, origins = {} }
				matches[key].origins[modId:gsub("^HandWraps", "")] = roll
			end
		end
	end

	local ordered = {}

	for _, match in pairs(matches) do
		table.insert(ordered, match)
	end

	-- Mods with more lines first, so a two-line mod is not split up.
	table.sort(ordered, function(a, b)
		if #a.indices ~= #b.indices then
			return #a.indices > #b.indices
		end

		return a.indices[1] < b.indices[1]
	end)

	local taken, original, untraced = {}, {}, {}

	for _, match in ipairs(ordered) do
		local free = true

		for _, index in ipairs(match.indices) do
			free = free and not taken[index]
		end

		if free then
			local groups, originId, roll = {}, nil, nil

			for id, idRoll in pairs(match.origins) do
				groups[(id:gsub("%d+$", ""))] = true
				originId, roll = id, idRoll
			end

			local count = 0

			for _ in pairs(groups) do
				count = count + 1
			end

			local origin = count == 1 and data.itemMods.Item[originId]

			for _, index in ipairs(match.indices) do
				taken[index] = true

				if not origin then
					table.insert(untraced, lines[index])
				end
			end

			if origin then
				for _, modLine in ipairs(origin) do
					table.insert(original, rollLine(modLine, roll))
				end
			end
		end
	end

	return original, untraced
end

-- Each explicit mod line's tier among the mods of its kind that can roll on
-- the item's base, 1 being the best: the line (with the other lines of its
-- mod) is matched to a mod by text and values, and ranked by level within
-- its group. Keyed by line text; mods PoB has no data for have no tier.
local function modTiers(item)
	local lines = {}

	for _, modLine in ipairs(item.explicitModLines) do
		table.insert(lines, modLine.line)
	end

	local levels, found = {}, {}
	-- Jewels, flasks and charms roll from mod lists of their own.
	local modLists = { Jewel = data.itemMods.Jewel, Flask = data.itemMods.Flask, Charm = data.itemMods.Charm }
	local subType = item.base.subType == "Charm" and "Charm"

	for _, mod in pairs(modLists[subType or item.type] or data.itemMods.Item) do
		if mod.group and item:GetModSpawnWeight(mod) > 0 then
			local key = mod.group .. ":" .. mod.type
			levels[key] = levels[key] or {}
			table.insert(levels[key], mod.level)
			local used = {}

			for _, modLine in ipairs(mod) do
				local match, roll

				for index, line in ipairs(lines) do
					roll = not used[index] and rollWithin(line, modLine)

					if roll then
						match = index
						break
					end
				end

				if not match then
					used = nil
					break
				end

				used[match] = roll
			end

			for index, roll in pairs(used or {}) do
				-- A mod matching more lines explains them better.
				if not found[index] or #mod > found[index].lines then
					found[index] = { key = key, level = mod.level, lines = #mod, roll = roll }
				end
			end
		end
	end

	local tiers = {}

	for index, match in pairs(found) do
		local tier = 1

		for _, level in ipairs(levels[match.key]) do
			if level > match.level then
				tier = tier + 1
			end
		end

		tiers[lines[index]] = { tier = tier, tiers = #levels[match.key], roll = match.roll }
	end

	return tiers
end

-- What a price check searches for: a unique by name, or an item's category,
-- defences and mods, matched to trade stats the way PoB's "Buy similar" does.
local function describeForPrice(item, slotName)
	local unique = item.rarity == "UNIQUE" or item.rarity == "RELIC"
	local mods, unsearchable, defences = {}, {}, {}
	local fistsOfStone = item.baseName:match("Fists of Stone$") ~= nil
	local searched = item
	local note
	local weapon

	-- The trade site compares weapon damage and defences at 20% quality, as a
	-- buyer can raise it.
	local full = item

	if (item.quality or 0) < 20 then
		full = new("Item", item:BuildRaw())
		full.quality = 20
		full:BuildModList()
	end

	-- Attack weapons are compared by their damage, which PoB calculates.
	if full.weaponData and full.weaponData[1] and not unique then
		local stats = full.weaponData[1]
		weapon = {
			physicalDps = stats.PhysicalDPS or 0,
			elementalDps = stats.ElementalDPS or 0,
			totalDps = stats.TotalDPS or 0,
			critChance = stats.CritChance or 0,
			attackRate = stats.AttackRate or 0,
		}
	end

	if fistsOfStone and not unique then
		-- Search for the gloves as they were, on any base: the original base and
		-- its defences are not known.
		local original, untraced = untransformFistsOfStone(item)
		searched = new("Item", "Rarity: RARE\nPrice Check\nStocky Mitts\nImplicits: 0\n" .. table.concat(original, "\n"))
		note = "Fists of Stone gloves are priced as the gloves they were, on any base, with each mod turned back into the one it came from"

		for _, line in ipairs(untraced) do
			table.insert(unsearchable, line .. " (not traceable to one original mod)")
		end
	end

	if not unique then
		local tiers = modTiers(searched)
		local entries = buySimilar.addModEntries(searched, {
			{ list = searched.enchantModLines, type = "enchant" },
			{ list = searched.implicitModLines, type = "implicit" },
			{ list = searched.explicitModLines, type = "explicit" },
		})

		for _, entry in ipairs(entries) do
			local lines, tier = {}, nil

			for _, line in ipairs(entry.formattedLines) do
				local plain = stripColors(line)
				table.insert(lines, plain)

				-- An aggregated entry is as good as its best line.
				if tiers[plain] and (not tier or tiers[plain].tier < tier.tier) then
					tier = tiers[plain]
				end
			end

			local text = table.concat(lines, ", ")

			if #entry.tradeIds == 0 then
				table.insert(unsearchable, text)
			else
				table.insert(mods, {
					text = text,
					ids = entry.tradeIds,
					value = entry.value,
					invert = entry.invert or false,
					option = entry.isOption,
					kind = entry.type,
					tier = tier and tier.tier,
					tiers = tier and tier.tiers,
					roll = tier and tier.roll,
				})
			end
		end

		for key, id in pairs({ Armour = "ar", Evasion = "ev", EnergyShield = "es" }) do
			local value = not fistsOfStone and full.armourData and full.armourData[key]

			if value and value > 0 then
				defences[id] = value
			end
		end
	end

	return {
		slot = slotName,
		name = item.title or item.name,
		base = item.baseName,
		-- The base to search by, when it is a real one.
		tradeBase = not fistsOfStone and item.baseName or nil,
		note = note,
		rarity = item.rarity,
		unique = unique,
		category = (tradeHelpers.getTradeCategory(slotName, item)),
		corrupted = item.corrupted or false,
		mods = mods,
		unsearchable = unsearchable,
		defences = defences,
		itemLevel = item.itemLevel,
		weapon = weapon,
	}
end

-- A pasted item, described for a price check.
function poe2.priceItem(text)
	local item = new("Item", text)

	if not item.base then
		error("PoB does not recognise this item; paste the full text copied in game with Ctrl+C", 0)
	end

	return describeForPrice(item, priceSlot(item))
end

-- Every item the build has equipped, described for price checks.
function poe2.equippedForPrice()
	local itemsTab = build.itemsTab
	itemsTab:UpdateSockets()
	local items = {}

	for _, slot in ipairs(itemsTab.orderedSlots) do
		local item = itemsTab.items[slot.selItemId]
		-- Sockets in items (Abyss-style) are priced with the item holding them.
		local inItem = slot.slotName:find("Jewel Socket") ~= nil
		local active = not slot.inactive and (slot.nodeId ~= nil or slot.shown())

		if item and item.base and active and not inItem then
			table.insert(items, describeForPrice(item, slot.slotName))
		end
	end

	return items
end

-- Trade listings (the fetch responses' JSON) converted to items the way PoB's
-- own trade window does it, and calculated in a slot.
function poe2.evaluateListings(slotName, bodies)
	local name = findSlot(slotName)
	local requests = new("TradeQueryRequests")
	local listings = {}

	for _, body in ipairs(bodies) do
		requests:FetchResultBlock("", function(items, err)
			if err then
				error(err, 0)
			end

			for _, entry in ipairs(items) do
				table.insert(listings, entry)
			end
		end)
		table.remove(requests.requestQueue.fetch).callback(body)
	end

	local calcFunc, baseOutput = build.calcsTab:GetMiscCalculator()
	local results = {}

	for _, listing in ipairs(listings) do
		local item = new("Item", listing.item_string)

		if item.base then
			local output = calcFunc({ repSlotName = name, repItem = item })
			local requirements = item.requirements or {}
			local result = impact(baseOutput, output, 1)
			result.id = listing.id
			result.name = item.name
			result.amount = listing.amount
			result.currency = listing.currency
			result.seller = listing.trader
			result.whisper = listing.whisper
			result.itemText = listing.item_string
			-- PoB calculates items the character cannot wear without complaint.
			result.meetsRequirements = (output.Str or 0) >= (requirements.str or 0)
				and (output.Dex or 0) >= (requirements.dex or 0)
				and (output.Int or 0) >= (requirements.int or 0)
			table.insert(results, result)
		end
	end

	return results
end
