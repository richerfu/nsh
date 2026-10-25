import { Input, Text, View, mount, signal, type YNode } from "nsh";
import { Clipboard } from "nsh/clipboard";

function App() {
  let editor!: YNode;
  const value = signal("ab😀\né中");
  const selection = signal("0:0");
  const focus = signal(false);
  const changes = signal(0);
  const visible = signal(true);
  const editable = signal(true);
  const multiline = signal(true);
  const submit = signal(false);
  const submitted = signal(0);
  const clipboard = signal("unknown");
  const action = (id: string, title: string, run: () => void) =>
    <View testID={id} onPress={run} style={{ flex: 1, height: 36, backgroundColor: "#dbeafe", padding: 6 }}><Text>{title}</Text></View>;
  return <View style={{ flex: 1, padding: 16, gap: 10, backgroundColor: "white" }}>
    <Text>Secure multiline input</Text>
    <Text testID={`password-state:${value().length}:${value().split("\n").length}:${changes()}:${focus()}`}>Length {value().length}; lines {value().split("\n").length}; changes {changes()}</Text>
    <Text testID={`password-selection:${selection()}`}>Selection {selection()}</Text>
    <If when={visible()}>
      <Input ref={editor} testID="password-editor" masked multiline={multiline()} value={value()} readonly={!editable()} submitBehavior={submit() ? "blurAndSubmit" : "newline"}
        accessibilityLabel="Multiline password" placeholder="Multiline password" maxLength={80}
        onChange={text => { value.set(text); changes.set(changes() + 1); }}
        onSelectionChange={event => selection.set(`${event.start}:${event.end}`)}
        onFocus={() => focus.set(true)} onBlur={() => focus.set(false)}
        onSubmit={() => submitted.set(submitted() + 1)}
        style={{ height: 110, padding: 8, fontSize: 24, lineHeight: 32, borderWidth: 1, borderColor: "#64748b" }} />
    </If>
    <View style={{ flexDirection: "row", gap: 8 }}>
    {action("password-focus", "Focus", () => editor.focus())}
    {action("password-emoji", "Select emoji", () => { editor.focus(); editor.setSelection(2, 4); })}
    {action("password-end", "End", () => { editor.focus(); editor.setSelection(value().length); })}
    </View>
    <View style={{ flexDirection: "row", gap: 8 }}>
    {action("password-blur", "Blur", () => editor.blur())}
    {action("password-reset", "Reset", () => value.set("ab😀\né中"))}
    {action(`password-editable:${editable()}`, "Editable", () => editable.set(!editable()))}
    </View>
    <View style={{ flexDirection: "row", gap: 8 }}>
    {action("password-retire", "Remove", () => { visible.set(false); focus.set(false); })}
    {action("password-remount", "Remount", () => visible.set(true))}
    {action(`password-submit:${submit()}`, "Return behavior", () => submit.set(!submit()))}
    </View>
    <View style={{ flexDirection: "row", gap: 8 }}>
    {action("password-clipboard-seed", "Seed clipboard", () => { void Clipboard.writeText("paste😀\nline"); })}
    {action("password-clipboard-read", "Check clipboard", async () => clipboard.set((await Clipboard.readText()) === "paste😀\nline" ? "seed" : "changed"))}
    </View>
    {action(`password-multiline:${multiline()}`, "Toggle multiline", () => multiline.set(!multiline()))}
    <Text testID={`password-submitted:${submitted()}`}>Submitted {submitted()}</Text>
    <Text testID={`password-clipboard:${clipboard()}`}>Clipboard {clipboard()}</Text>
    <Input testID="ordinary-editor" placeholder="Ordinary editor" style={{ height: 44, borderWidth: 1, padding: 8 }} />
  </View>;
}
mount(App);
