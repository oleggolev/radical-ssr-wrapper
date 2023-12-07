/**
 * - RADICAL orchestrator wrapper around SSR.
 */

import { get_index_rw_set, get_index_template, get_about_template } from './radical-ssr/pkg/ssr_bench';

const cache = await caches.open('RADICAL_SSR_WRAPPER');
let env = {}
let logger = console.log

function keyToCacheKey(key) {
	return new Request(`http://radicalcache/key/${key}`);
}

function valueToCacheValue(value) {
	let cacheResp = new Response(JSON.stringify({ value }));
	cacheResp.headers.append("Cache-Control", "max-age=1000");
	cacheResp.headers.append("Cache-Control", "public");
	cacheResp.headers.append("Content-Type", "application/json");
	return cacheResp;
}

async function checkCacheVersions(keys) {
	let kvMap = {};
	const randomValues = new Uint32Array(keys.length)
	await Promise.all(
		keys.map(async (key, idx) => {
			logger("Going to check the cache for key", key)
			const cacheKey = keyToCacheKey(key)
			let cacheValue = await cache.match(cacheKey)
			let version = 0
			if (cacheValue != undefined) {
				let value = await cacheValue.json()
				let missProb = (env.COLD_MISS_PROB + env.CAP_MISS_PROB) * 100
				let randomValue = randomValues[idx] / (0xffffffff + 1);
				let compRandom = Math.floor(randomValue * (100 - 0 + 1));
				if (compRandom <= missProb) {
					logger(`Triggering intentional miss on ${key} (${compRandom} vs ${missProb})`)
					cacheValue = undefined;
				}
				version = value["value"].Version
			}
			if (cacheValue == undefined) {
				logger(`${key} not present in the storage system (or we triggered an intentional miss)`)
				version = -1
			}
			logger(`Located version ${version} of ${key}`)
			kvMap[key] = version
		})
	);
	return kvMap;
}

async function updateCache(key, value) {
	const cacheKey = keyToCacheKey(key)
	const cacheValue = valueToCacheValue(value)
	await cache.put(cacheKey, cacheValue)
}

async function handleConsistencyCheck(versions, args) {
	let reqData = {
		versions: versions,
		function: env.BACKUP,
		args: args
	}
	logger("Sending consistency check for", versions, "to", env.REMOTE_URL, "with remote func", env.BACKUP);
	return fetch(env.REMOTE_URL, {
		method: "POST",
		headers: {
			"Content-Type": "application/json",
		},
		body: JSON.stringify(reqData)
	}).then(async resp => {
		return resp.json()
	});
}

async function orchestrate(request) {
	const start = performance.now()

	// Extract search params or set defaults if absent.
	const url = new URL(request.url);
	let page_num = url.searchParams.get('page_num');
	let page_size = url.searchParams.get('page_size');
	if (page_num == undefined) {
		page_num = 1;
	}
	if (page_size == undefined) {
		page_size = 20;
	}
	const args = {
		"page_num": page_num,
		"page_size": page_size,
	};

	// Get the current versions of the keys in question
	const versionStart = performance.now()
	let rw_set = await get_index_rw_set(page_num, page_size);
	let checkVersions = await checkCacheVersions(rw_set);
	let versionEnd = performance.now()
	logger("Result of version check:", checkVersions, "took", versionEnd - versionStart)

	// Kick off the consistency check before running the function
	let consistencyPromise = null
	if (env.DO_CONSISTENCY_CHECK) {
		consistencyPromise = handleConsistencyCheck(checkVersions, args);
	} else {
		logger("Skipping consistency check that would have used:", env.BACKUP, env.REMOTE_URL, args)
	}

	// Now run the function while we have the http request to the consistency check fired off
	logger("Args to function", args)
	let functionResult = await target_function(rw_set)
	logger("Result of function invocation", functionResult)
	const consistencyStart = performance.now()
	let consistencyResult = consistencyPromise != null ? await consistencyPromise : { checkResult: true };
	const consistencyEnd = performance.now()
	logger("Result of consistency check", consistencyResult, "took", consistencyEnd - consistencyStart);
	let endResult = {
		success: consistencyResult.checkResult
	}
	if (consistencyResult.checkResult) {
		endResult.result = functionResult
	} else {
		endResult.result = consistencyResult.result
	}
	logger("End result we should return to the client", endResult)
	if (!consistencyResult.checkResult) {
		logger("Consistency result failed")
		let updateStart = performance.now()
		await Promise.all(consistencyResult.updatedKeys.map(async (obj) => {
			let { ID, Key, Value, Version } = obj
			await updateCache(Key, { ID, Key, Value, Version })
			logger(`Updated ${ID}`)
		}));
		let updateEnd = performance.now()
		logger("Finished updating the keys", updateEnd - updateStart);
	}
	// If we failed, we also need to update storage
	let end = performance.now()
	logger("Returning to user", end - start)
	return new Response(JSON.stringify(endResult.result), {
		headers: {
			"content-type": "application/json;charset=UTF-8",
		},
	});
}

async function target_function(rw_set) {
	// Get all posts needed to satisfy the request. If some post is not in Cache, return false and rely on the datacenter result.
	const posts = await Promise.all(
		rw_set.map(async key => {
			let cacheValue = await cache.match(keyToCacheKey(key));
			if (cacheValue == undefined) {
				const err_msg = "Cannot run target_function locally: key {" + key + "} not present in cache";
				logger(err_msg)
				throw new Error(err_msg);
			} else {
				return await cacheValue.json();
			}
		})
	).catch(_err => { return false; });;

	// At this point, we got all cache hits, so generate the page and return result.
	let funcStart = performance.now();
	let template = get_index_template(posts);
	let funcEnd = performance.now();
	logger('get_index_template runtime', funcEnd - funcStart);
	return template;
}

export default {
	// Assumes that all requests that come in will be served with the index page.
	async fetch(request, environment, _ctx) {
		if (!environment.DEBUGPRINT) {
			logger = function () { }
		}
		logger("Starting function!")
		env = environment
		let orchStart = performance.now()
		let orchResult = await orchestrate(request)
		let orchEnd = performance.now()
		logger("Got orchestrator result in", orchEnd - orchStart)
		return orchResult;
	},
};
