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
	    confirmRerun: boolean;
	    optionIsMeta: boolean;
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
	        this.confirmRerun = source["confirmRerun"];
	        this.optionIsMeta = source["optionIsMeta"];
	        this.aiEnabled = source["aiEnabled"];
	        this.aiModel = source["aiModel"];
	        this.aiShareOutput = source["aiShareOutput"];
	    }
	}

}

