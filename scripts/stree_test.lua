math.randomseed(os.time())

local number_of_ids = 50000
local number_of_features_per_id = 5
local number_of_random_bu = 10

local feature_ids = {}

for i = 1, number_of_ids do
  feature_ids[i] = string.format("feature_%d", math.random(1, 500))
end

local counter = 1
local threads = {}

function setup(thread)
	thread:set("id", counter)
	table.insert(threads, thread)
	counter = counter + 1
end

local function generate_update_payload()
	local batch_updates = {}
	for i = 1, number_of_ids do
		local curr_feat_up = {}
		for k = 1, number_of_features_per_id do
			curr_feat_up[k] = math.random(1, 100)
		end
		batch_updates[i] = string.format('"feature_%d": [%s]', i, table.concat(curr_feat_up, ","))
	end
	return string.format("{%s}", table.concat(batch_updates, ","))
end

function init(args)
	if id == 1 then
    print("Creating fake requests")
		for i = 1, number_of_random_bu do
			local update = generate_update_payload()
			local filename = string.format("requests/req_%d.json", i)
			local file = io.open(filename, "w")
			if file then
				file:write(update)
				file:close()
			end
		end
	end
end

-- Helper function to generate a JSON payload for /feature endpoint
local function generate_feature_payload()
	local random_id = feature_ids[math.random(#feature_ids)]
	return string.format('{"id": "%s"}', random_id)
end

function request()
	-- Randomly pick an endpoint to hit
  local endpoint
	if id == 1 then
		endpoint = "/batch_update"
	else
		endpoint = "/feature"
	end
	if endpoint == "/feature" then
		return wrk.format("GET", endpoint, { ["Content-Type"] = "application/json" }, generate_feature_payload())
	elseif endpoint == "/batch_update" then
		local fid = math.random(number_of_random_bu)
		local payload = string.format('{"path": "requests/req_%d.json"}', fid)
		return wrk.format("POST", endpoint, { ["Content-Type"] = "application/json" }, payload)
	end
end

function delay()
	if id == 1 then
		return 10000
	else
		return 0
	end
end