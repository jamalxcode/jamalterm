export namespace ai {
	
	export class Account {
	    signedIn: boolean;
	    profile: string;
	    workspace: string;
	
	    static createFrom(source: any = {}) {
	        return new Account(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.signedIn = source["signedIn"];
	        this.profile = source["profile"];
	        this.workspace = source["workspace"];
	    }
	}
	export class ProviderInfo {
	    id: string;
	    name: string;
	    label: string;
	    company: string;
	    keyUrl: string;
	    keyWhere: string;
	    keyHint: string;
	    billing: string;
	    modelField: string;
	    autoModel: string;
	    signIn: string;
	
	    static createFrom(source: any = {}) {
	        return new ProviderInfo(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.id = source["id"];
	        this.name = source["name"];
	        this.label = source["label"];
	        this.company = source["company"];
	        this.keyUrl = source["keyUrl"];
	        this.keyWhere = source["keyWhere"];
	        this.keyHint = source["keyHint"];
	        this.billing = source["billing"];
	        this.modelField = source["modelField"];
	        this.autoModel = source["autoModel"];
	        this.signIn = source["signIn"];
	    }
	}
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
	    opacity: number;
	    blur: boolean;
	    cursorBlink: boolean;
	    scrollback: number;
	    shell: string;
	    shellIntegration: boolean;
	    arrowBrowse: boolean;
	    restoreSession: boolean;
	    notifyLongCommands: boolean;
	    notifyAfterSeconds: number;
	    screenReader: boolean;
	    promptKeys: string;
	    confirmRerun: boolean;
	    optionIsMeta: boolean;
	    keepProblemLog: boolean;
	    autoUpdate: boolean;
	    aiEnabled: boolean;
	    aiProvider: string;
	    aiModel: string;
	    openaiModel: string;
	    deepseekModel: string;
	    xaiModel: string;
	    openrouterModel: string;
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
	        this.opacity = source["opacity"];
	        this.blur = source["blur"];
	        this.cursorBlink = source["cursorBlink"];
	        this.scrollback = source["scrollback"];
	        this.shell = source["shell"];
	        this.shellIntegration = source["shellIntegration"];
	        this.arrowBrowse = source["arrowBrowse"];
	        this.restoreSession = source["restoreSession"];
	        this.notifyLongCommands = source["notifyLongCommands"];
	        this.notifyAfterSeconds = source["notifyAfterSeconds"];
	        this.screenReader = source["screenReader"];
	        this.promptKeys = source["promptKeys"];
	        this.confirmRerun = source["confirmRerun"];
	        this.optionIsMeta = source["optionIsMeta"];
	        this.keepProblemLog = source["keepProblemLog"];
	        this.autoUpdate = source["autoUpdate"];
	        this.aiEnabled = source["aiEnabled"];
	        this.aiProvider = source["aiProvider"];
	        this.aiModel = source["aiModel"];
	        this.openaiModel = source["openaiModel"];
	        this.deepseekModel = source["deepseekModel"];
	        this.xaiModel = source["xaiModel"];
	        this.openrouterModel = source["openrouterModel"];
	        this.aiShareOutput = source["aiShareOutput"];
	    }
	}

}

