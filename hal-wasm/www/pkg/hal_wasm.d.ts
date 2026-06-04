declare namespace wasm_bindgen {
    /* tslint:disable */
    /* eslint-disable */

    export function feed_cursor_key(direction: number): void;

    export function feed_input_byte(byte: number): void;

    export function feed_toggle_angle(): void;

    export function feed_toggle_mode(): void;

    export function get_framebuffer(): Uint8Array;

    export function get_framebuffer_ptr(): number;

    export function get_mode(): number;

    export function get_serial_output(): string;

    export function init(): void;

    export function tick(): void;

}
declare type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

declare interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly feed_cursor_key: (a: number) => void;
    readonly feed_input_byte: (a: number) => void;
    readonly feed_toggle_angle: () => void;
    readonly feed_toggle_mode: () => void;
    readonly get_framebuffer: () => [number, number];
    readonly get_framebuffer_ptr: () => number;
    readonly get_mode: () => number;
    readonly get_serial_output: () => [number, number];
    readonly init: () => void;
    readonly tick: () => void;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
declare function wasm_bindgen (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
