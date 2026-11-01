export namespace ai {
	
	export class Request {
	    id: string;
	    chat: string;
	    mode: string;
	    prompt: string;
	    context: string;
	    shell: string;
	
	    static createFrom(source: any = {}) {
	        return new Request(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.id = source["id"];
	        this.chat = source["chat"];
	        this.mode = source["mode"];
	        this.prompt = source["prompt"];
	        this.context = source["context"];
	        this.shell = source["shell"];
	    }
	}

}

export namespace main {
	
	export class UpdateInfo {
	    available: boolean;
	    current: string;
	    latest: string;
	    notes: string;
	    url: string;
	    canInstall: boolean;
	    reason: string;
	
	    static createFrom(source: any = {}) {
	        return new UpdateInfo(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.available = source["available"];
	        this.current = source["current"];
	        this.latest = source["latest"];
	        this.notes = source["notes"];
	        this.url = source["url"];
	        this.canInstall = source["canInstall"];
	        this.reason = source["reason"];
	    }
	}

}

export namespace settings {
	
	export class Settings {
	    theme: string;
	    fontFamily: string;
	    fontSize: number;
	    lineHeight: number;
	    cursorStyle: string;
	    cursorBlink: boolean;
	    scrollback: number;
	    shell: string;
	    shellIntegration: boolean;
	    arrowBrowse: boolean;
	    confirmRerun: boolean;
	    optionIsMeta: boolean;
	    autoUpdate: boolean;
	    aiEnabled: boolean;
	    aiModel: string;
	    aiShareOutput: boolean;
	
	    static createFrom(source: any = {}) {
	        return new Settings(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.theme = source["theme"];
	        this.fontFamily = source["fontFamily"];
	        this.fontSize = source["fontSize"];
	        this.lineHeight = source["lineHeight"];
	        this.cursorStyle = source["cursorStyle"];
	        this.cursorBlink = source["cursorBlink"];
	        this.scrollback = source["scrollback"];
	        this.shell = source["shell"];
	        this.shellIntegration = source["shellIntegration"];
	        this.arrowBrowse = source["arrowBrowse"];
	        this.confirmRerun = source["confirmRerun"];
	        this.optionIsMeta = source["optionIsMeta"];
	        this.autoUpdate = source["autoUpdate"];
	        this.aiEnabled = source["aiEnabled"];
	        this.aiModel = source["aiModel"];
	        this.aiShareOutput = source["aiShareOutput"];
	    }
	}

}

