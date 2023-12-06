import * as imports from './ssr_bench_bg.js';

import wkmod from './ssr_bench_bg.wasm';
import * as nodemod from './ssr_bench_bg.wasm';
if (typeof process !== 'undefined' && process.release.name === 'node') {
    imports.__wbg_set_wasm(nodemod);
} else {
    const instance = new WebAssembly.Instance(wkmod, { './ssr_bench_bg.js': imports });
    imports.__wbg_set_wasm(instance.exports);
}

export * from './ssr_bench_bg.js';
