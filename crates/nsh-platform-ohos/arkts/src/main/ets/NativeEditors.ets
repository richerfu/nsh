import { BuilderNode } from "@ohos.arkui.node";
import { FrameCallback } from "@ohos.arkui.UIContext";
import {
  BridgePluginContext,
} from "@ohos-rs/ability";

export interface EditorSettings {
  targetId: number;
  multiline: boolean;
  secure: boolean;
  text: string;
  count: number;
  width: number;
  height: number;
  placeholder: string;
  fontSize: number;
  fontWeight: number;
  fontFamily?: string | null;
  italic: boolean;
  color: number;
  placeholderColor: number;
  caretColor: number;
  selectionColor: number;
  editable: boolean;
  hitTest: number;
  align: number;
  rtl: boolean;
  capitalize: number;
  keyboard: string;
  enterKey: string;
  blurOnSubmit: boolean;
  maxLength?: number | null;
  autoFill: boolean;
  contentType?: string | null;
  autoFocus: boolean;
  accessibility?: EditorAccessibility | null;
}
interface EditorAccessibility {
  label: string;
  hint: string;
  hidden: boolean;
  accessible?: boolean | null;
  disabled: boolean;
}
export interface EditorCommand {
  targetId: number;
  kind: string;
  start: number;
  end: number;
}
interface EditorReceipt { accepted: boolean; count: number; }
interface Acknowledgement {
  accepted: boolean;
}
const inputTypes: Map<string, InputType> = new Map([
  ["normal", InputType.Normal],
  ["number", InputType.Number],
  ["decimal", InputType.NUMBER_DECIMAL],
  ["phone", InputType.PhoneNumber],
  ["email", InputType.Email],
  ["password", InputType.Password],
  ["number-password", InputType.NUMBER_PASSWORD],
]);
const areaTypes: Map<string, TextAreaType> = new Map([
  ["normal", TextAreaType.NORMAL],
  ["number", TextAreaType.NUMBER],
  ["decimal", TextAreaType.NUMBER_DECIMAL],
  ["phone", TextAreaType.PHONE_NUMBER],
  ["email", TextAreaType.EMAIL],
]);
const enterTypes: Map<string, EnterKeyType> = new Map([
  ["go", EnterKeyType.Go],
  ["next", EnterKeyType.Next],
  ["search", EnterKeyType.Search],
  ["send", EnterKeyType.Send],
  ["newline", EnterKeyType.NEW_LINE],
  ["done", EnterKeyType.Done],
]);
const contentTypes: Map<string, ContentType> = new Map([
  ["username", ContentType.USER_NAME],
  ["password", ContentType.PASSWORD],
  ["new-password", ContentType.NEW_PASSWORD],
  ["email", ContentType.EMAIL_ADDRESS],
  ["tel", ContentType.FULL_PHONE_NUMBER],
  ["name", ContentType.PERSON_FULL_NAME],
  ["given-name", ContentType.PERSON_FIRST_NAME],
  ["family-name", ContentType.PERSON_LAST_NAME],
  ["nickname", ContentType.NICKNAME],
  ["street-address", ContentType.FULL_STREET_ADDRESS],
  ["address-line2", ContentType.DETAIL_INFO_WITHOUT_STREET],
  ["city", ContentType.CITY_ADDRESS],
  ["region", ContentType.PROVINCE_ADDRESS],
  ["country", ContentType.COUNTRY_ADDRESS],
  ["credit-card", ContentType.BANK_CARD_NUMBER],
]);
function inputType(value: string): InputType {
  const result = inputTypes.get(value);
  if (result === undefined) {
    throw new Error("Invalid editor keyboard");
  }
  return result;
}
function areaType(value: string): TextAreaType {
  const result = areaTypes.get(value);
  if (result === undefined) {
    throw new Error("Invalid multiline editor keyboard");
  }
  return result;
}
function enterType(value: string): EnterKeyType {
  const result = enterTypes.get(value);
  if (result === undefined) {
    throw new Error("Invalid editor return key");
  }
  return result;
}
function contentType(value?: string | null): ContentType | undefined {
  if (value == null) {
    return undefined;
  }
  const result = contentTypes.get(value);
  if (result === undefined) {
    throw new Error("Invalid editor autofill type");
  }
  return result;
}

class EditorEvent {
  readonly kind: string;
  readonly text: string;
  readonly count: number;
  readonly start: number;
  readonly end: number;
  readonly width: number;
  readonly height: number;
  constructor(
    entry: EditorEntry,
    kind: string,
    text: string,
    start = 0,
    end = 0,
    width = 0,
    height = 0,
  ) {
    this.kind = kind;
    this.text = text;
    this.count = entry.count;
    this.start = start;
    this.end = end;
    this.width = width;
    this.height = height;
  }
}

class EditorModifier implements AttributeModifier<TextInputAttribute | TextAreaAttribute> {
  private entry: EditorEntry;
  constructor(entry: EditorEntry) {
    this.entry = entry;
  }
  applyNormalAttribute(node: TextInputAttribute | TextAreaAttribute): void {
    const s = this.entry.settings;
    node
      .width(s.width)
      .height(s.height)
      .padding(0)
      .borderWidth(0)
      // The outer NSH node owns corners; theme rounding would clip this content box.
      .borderRadius(0)
      .backgroundColor(Color.Transparent)
      .fontSize(s.fontSize)
      .fontWeight(s.fontWeight)
      .fontFamily(s.fontFamily ?? "HarmonyOS Sans")
      .fontStyle(s.italic ? FontStyle.Italic : FontStyle.Normal)
      .fontColor(s.color)
      .placeholderColor(s.placeholderColor)
      .caretColor(s.caretColor)
      .selectedBackgroundColor(s.selectionColor)
      .enabled(s.editable && s.accessibility?.disabled !== true)
      .accessibilityText(s.accessibility?.label ?? "")
      .accessibilityDescription(s.accessibility?.hint ?? "")
      .accessibilityLevel(
        s.accessibility?.hidden === true
          ? "no-hide-descendants"
          : s.accessibility?.accessible === false
            ? "no"
            : "yes",
      )
      .textAlign(s.align as TextAlign)
      .direction(s.rtl ? Direction.Rtl : Direction.Ltr)
      .autoCapitalizationMode(s.capitalize as AutoCapitalizationMode)
      .enterKeyType(enterType(s.enterKey))
      .maxLength(s.maxLength ?? 2147483647)
      .enableAutoFill(s.autoFill)
      .contentType(contentType(s.contentType))
      .hitTestBehavior(s.hitTest as HitTestMode);
    if (s.multiline && !s.secure) {
      (node as TextAreaAttribute).type(areaType(s.keyboard));
    } else {
      (node as TextInputAttribute)
        .type(inputType(s.keyboard))
        .showPasswordIcon(false)
        .showUnderline(false);
    }
  }
}

@Builder
function editorContent(entry: EditorEntry) {
  if (entry.settings.multiline && !entry.settings.secure) {
    TextArea({ text: entry.text, placeholder: entry.settings.placeholder, controller: entry.area })
      .id(`nsh-editor-${entry.settings.targetId}`)
      .attributeModifier(new EditorModifier(entry))
      .onChange((text: string) => entry.change(text))
      .onFocus(() => entry.emit("focus"))
      .onBlur(() => entry.emit("blur"))
      .onSubmit((_key: EnterKeyType, event?: SubmitEvent) => entry.submit(event))
      .onTextSelectionChange((start: number, end: number) =>
        entry.emit("selection", "", start, end),
      )
      .onWillInsert((value: InsertValue): boolean => { entry.editKey(value.insertValue); return true; })
      .onWillDelete((): boolean => { entry.editKey("Backspace"); return true; })
      .onAppear(() => entry.appeared());
  } else {
    TextInput({ text: entry.text, placeholder: entry.settings.placeholder, controller: entry.line })
      .id(`nsh-editor-${entry.settings.targetId}`)
      .attributeModifier(new EditorModifier(entry))
      .onChange((text: string) => entry.change(text))
      .onFocus(() => entry.emit("focus"))
      .onBlur(() => entry.emit("blur"))
      .onSubmit((_key: EnterKeyType, event: SubmitEvent) => entry.submit(event))
      .onTextSelectionChange((start: number, end: number) =>
        entry.emit("selection", "", start, end),
      )
      .onWillInsert((value: InsertValue): boolean => { entry.editKey(value.insertValue); return true; })
      .onWillDelete((): boolean => { entry.editKey("Backspace"); return true; })
      .onAppear(() => entry.appeared());
  }
}

// Failed retirement cannot release a BuilderNode still borrowed by native code.
const quarantinedEditors: Set<EditorEntry> = new Set();
class EditorSizeCallback extends FrameCallback {
  private entry: EditorEntry;
  constructor(entry: EditorEntry) {
    super();
    this.entry = entry;
  }
  onIdle(_timeLeft: number): void {
    this.entry.measureSize();
  }
}
class EditorEntry {
  readonly node: BuilderNode<[EditorEntry]>;
  readonly line: TextInputController = new TextInputController();
  readonly area: TextAreaController = new TextAreaController();
  settings: EditorSettings;
  text: string;
  count: number;
  private updating: boolean = false;
  private closed: boolean = false;
  private retired: boolean = false;
  private disposed: boolean = false;
  private sdkBusy: boolean = false;
  private attached: boolean = false;
  private appearedPending: boolean = false;
  private disposePending: boolean = false;
  private focusedOnce: boolean = false;
  private context: BridgePluginContext;
  private sizePending: boolean = false;
  private contentWidth: number = -1;
  private contentHeight: number = -1;
  constructor(settings: EditorSettings, context: BridgePluginContext) {
    this.context = context;
    this.settings = settings;
    this.text = settings.text;
    this.count = settings.count;
    this.node = new BuilderNode<[EditorEntry]>(context.getUIContext());
  }
  create(): void {
    this.updating = true;
    this.sdkBusy = true;
    try {
      this.node.build(wrapBuilder(editorContent), this);
      if (this.closed || !this.context.isActive()) { throw new Error("Editor creation was cancelled"); }
      const response = this.context.invokeNativeSync(
        "native-node-ready",
        "ohos.node.NativeNodeReady",
        "ohos.node.Acknowledgement",
        { targetId: this.settings.targetId, content: this.node.getFrameNode() },
      ) as Acknowledgement;
      if (response.accepted !== true || this.closed) { throw new Error("Native editor target is no longer live"); }
      this.attached = true;
    } catch (error) {
      this.dispose();
      throw new Error(String(error));
    } finally {
      this.updating = false;
      this.sdkBusy = false;
      if (this.disposePending) { this.disposePending = false; this.release(); }
      this.autoFocus();
    }
  }
  update(settings: EditorSettings): void {
    if (this.closed) { throw new Error("Native editor is retired"); }
    if (
      settings.targetId !== this.settings.targetId ||
      settings.multiline !== this.settings.multiline
    ) {
      throw new Error("Editor replacement requires a new generation");
    }
    if (settings.count === this.count) { this.text = settings.text; }
    this.settings = settings;
    this.refresh();
  }
  private refresh(): void {
    this.updating = true;
    this.sdkBusy = true;
    try {
      this.node.update(this);
    } finally {
      this.updating = false;
      this.sdkBusy = false;
      if (this.disposePending) { this.disposePending = false; this.release(); }
      this.autoFocus();
    }
    this.scheduleSize();
  }
  // Both soft and hardware editing use these SDK callbacks. The native key
  // event path handles shortcuts/KeyDown independently and never emits KeyPress.
  editKey(key: string): void {
    for (const character of key === "Backspace" ? [key] : Array.from(key)) {
      this.emit("key", character);
    }
  }
  change(text: string): void {
    if (this.updating || this.closed || this.text === text) {
      return;
    }
    this.text = text;
    this.emit("change", text);
    this.scheduleSize();
  }
  submit(event?: SubmitEvent): void {
    if (!this.settings.blurOnSubmit) {
      event?.keepEditableState();
    }
    this.emit("submit");
  }
  private scheduleSize(): void {
    if (this.sizePending || this.closed) {
      return;
    }
    this.sizePending = true;
    this.context.getUIContext().postFrameCallback(new EditorSizeCallback(this));
  }
  measureSize(): void {
    this.sizePending = false;
    if (this.closed || !this.context.isActive()) {
      return;
    }
    const rect = this.settings.multiline && !this.settings.secure
      ? this.area.getTextContentRect()
      : this.line.getTextContentRect();
    if (rect.width !== this.contentWidth || rect.height !== this.contentHeight) {
      this.contentWidth = rect.width;
      this.contentHeight = rect.height;
      this.emit("size", "", 0, 0, rect.width, rect.height);
    }
  }
  emit(kind: string, text = "", start = 0, end = 0, width = 0, height = 0): void {
    if (!this.attached || this.closed || !this.context.isActive() || (this.updating && kind !== "focus" && kind !== "blur")) {
      return;
    }
    const response = this.context.invokeNativeSync(
      "editor-event", "nsh.arkui.EditorNotification", "nsh.arkui.EditorReceipt",
      { targetId: this.settings.targetId, event: new EditorEvent(this, kind, text, start, end, width, height) },
    ) as EditorReceipt;
    if (response.accepted) { this.count = response.count; }

  }
  appeared(): void {
    this.scheduleSize();
    this.appearedPending = true;
    this.autoFocus();
  }
  private autoFocus(): void {
    if (this.sdkBusy || !this.attached || this.closed || !this.context.isActive()) { return; }
    if (this.appearedPending && !this.focusedOnce && this.settings.autoFocus) {
      this.focusedOnce = true;
      this.context.getUIContext().getFocusController().requestFocus(`nsh-editor-${this.settings.targetId}`);
    }
  }
  command(command: EditorCommand): void {
    if (command.kind === "focus") {
      this.context.getUIContext().getFocusController().requestFocus(`nsh-editor-${this.settings.targetId}`);
    } else if (command.kind === "blur") {
      if (this.settings.multiline && !this.settings.secure) {
        this.area.stopEditing();
      } else {
        this.line.stopEditing();
      }
    } else if (command.kind === "selection") {
      if (this.settings.multiline && !this.settings.secure) {
        this.area.setTextSelection(command.start, command.end);
      } else {
        this.line.setTextSelection(command.start, command.end);
      }
    } else {
      throw new Error(`Unsupported editor command '${command.kind}'`);
    }
  }
  private release(): void {
    try { this.node.dispose(); this.disposed = true; quarantinedEditors.delete(this); }
    catch (error) { quarantinedEditors.add(this); throw new Error(String(error)); }
  }
  dispose(cleanup: BridgePluginContext = this.context): void {
    if (this.disposed) { return; }
    this.closed = true;
    try {
      if (!this.retired) {
        const response = cleanup.invokeNativeSync(
          "native-node-retiring", "ohos.node.NativeNodeRetiring", "ohos.node.Acknowledgement",
          { targetId: this.settings.targetId },
        ) as Acknowledgement;
        if (!response.accepted) { throw new Error("Native editor retirement was rejected"); }
        this.retired = true;
      }
      if (this.sdkBusy) { this.disposePending = true; } else { this.release(); }
    } catch (error) {
      quarantinedEditors.add(this);
      throw new Error(`Native editor retirement failed: ${String(error)}`);
    }
  }

}

export class NativeEditors {
  private entries: Map<number, EditorEntry> = new Map();
  remove(ids: number[], context: BridgePluginContext): void {
    for (const id of ids) { this.entries.get(id)?.dispose(context); this.entries.delete(id); }
  }
  update(states: EditorSettings[], commands: EditorCommand[], context: BridgePluginContext): void {
    for (const state of states) {
      if (!context.isActive()) { throw new Error("NSH SDK session is inactive"); }
      let entry = this.entries.get(state.targetId);
      if (entry) { entry.update(state); }
      else {
        entry = new EditorEntry(state, context);
        // Publish before SDK build: onAppear/onClosing may reenter synchronously.
        this.entries.set(state.targetId, entry);
        entry.create();
      }
    }
    for (const command of commands) { this.entries.get(command.targetId)?.command(command); }
  }
  dispose(context: BridgePluginContext): void {
    let failure: Error | undefined;
    for (const id of Array.from(this.entries.keys())) {
      const entry = this.entries.get(id)!;
      try { entry.dispose(context); this.entries.delete(id); }
      catch (error) { failure ??= new Error(String(error)); }
    }
    if (failure) { throw failure; }
  }
}
