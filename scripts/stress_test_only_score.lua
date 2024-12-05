math.randomseed(os.time())

local number_of_ids = 320000
local feature_ids = {}

for i = 1, number_of_ids do
  feature_ids[i] = string.format("feature_%d", math.random(1, number_of_ids))
end

local function generate_score_payload()
	local random_id = feature_ids[math.random(#feature_ids)]
	return string.format('{"f1": "%s", "f2": %d}', random_id, 1)
end

function request()
	-- Randomly pick an endpoint to hit
  return wrk.format("GET", "/score", { ["Content-Type"] = "application/json" }, generate_score_payload())
end
