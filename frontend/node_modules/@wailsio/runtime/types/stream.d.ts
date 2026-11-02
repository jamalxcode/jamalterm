/**
 * A stream connection. Implements the useful subset of the `WebSocket`
 * interface, so the same application code works against a real WebSocket in
 * server builds.
 *
 * One deliberate divergence from the standard: `binaryType` defaults to
 * `"arraybuffer"` rather than `"blob"`, because stream frames are always
 * binary and a Blob would force an extra async hop to read every message.
 * Setting it to `"blob"` behaves as the standard describes.
 */
export declare class WailsSocket extends EventTarget {
    static readonly CONNECTING = 0;
    static readonly OPEN = 1;
    static readonly CLOSING = 2;
    static readonly CLOSED = 3;
    readonly CONNECTING = 0;
    readonly OPEN = 1;
    readonly CLOSING = 2;
    readonly CLOSED = 3;
    /** The stream name this connection was opened against. */
    readonly name: string;
    readonly url: string;
    /** Always empty: subprotocols and extensions are not negotiated. */
    readonly protocol = "";
    readonly extensions = "";
    binaryType: "arraybuffer" | "blob";
    readyState: number;
    onopen: ((ev: Event) => void) | null;
    onmessage: ((ev: MessageEvent) => void) | null;
    onclose: ((ev: CloseEvent) => void) | null;
    onerror: ((ev: Event) => void) | null;
    private _buffered;
    private _chain;
    private _pending;
    private _flushing;
    private _openAbort;
    private _sendAbort;
    /** Bytes queued by send() that have not yet reached Go. */
    get bufferedAmount(): number;
    constructor(name: string);
    /**
     * Send a frame to Go. Accepts anything a WebSocket does; strings are
     * encoded as UTF-8, since Go receives every frame as a byte slice.
     */
    send(data: string | ArrayBufferLike | ArrayBufferView | Blob): void;
    /** Close the connection. Go's handler sees Receive fail and returns. */
    close(code?: number, reason?: string): void;
}
/**
 * Connect to a stream declared in Go with `app.HandleStream(name, handler)`.
 *
 * Returns immediately with `readyState === CONNECTING`; listen for `open`, or
 * just call {@link WailsSocket.send} once open. Creating a connection at module
 * scope is supported and is the intended shape for generated bindings.
 */
export declare function Stream(name: string): WailsSocket | WebSocket;
/**
 * The object-shaped view of a stream returned by {@link JSONStream}: `send`
 * takes a value to stringify, and `ev.data` on a message is the parsed result.
 */
export interface JSONSocket extends Omit<WailsSocket, "send" | "onmessage" | "addEventListener" | "removeEventListener"> {
    send(value: unknown): void;
    onmessage: ((ev: MessageEvent<any>) => void) | null;
    addEventListener(type: "message", listener: ((this: JSONSocket, ev: MessageEvent<any>) => any) | EventListenerObject | null, options?: boolean | AddEventListenerOptions): void;
    addEventListener(type: string, listener: EventListenerOrEventListenerObject | null, options?: boolean | AddEventListenerOptions): void;
    removeEventListener(type: "message", listener: ((this: JSONSocket, ev: MessageEvent<any>) => any) | EventListenerObject | null, options?: boolean | EventListenerOptions): void;
    removeEventListener(type: string, listener: EventListenerOrEventListenerObject | null, options?: boolean | EventListenerOptions): void;
}
/**
 * A {@link Stream} that speaks objects instead of bytes.
 *
 * `send(value)` marshals with `JSON.stringify`, and `ev.data` in an `onmessage`
 * handler is the parsed object rather than an `ArrayBuffer`. Pairs with
 * `StreamConn.SendJSON` / `ReceiveJSON` on the Go side.
 *
 * ```js
 * const s = JSONStream("telemetry");
 * s.onmessage = (ev) => console.log(ev.data.temperature);
 * s.onopen    = () => s.send({ subscribe: "sensors" });
 * ```
 *
 * A frame that is not valid JSON raises an `error` event and is dropped, rather
 * than throwing from the poll loop and taking the connection down with it.
 */
export declare function JSONStream(name: string): JSONSocket;
