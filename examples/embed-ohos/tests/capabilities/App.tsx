import { mount, signal, Text, View } from "nsh";
import { Canvas } from "nsh/canvas";
import { AlertDialog, createNavStack, NavPage, NavStack } from "nsh/base";

function App() {
  const nav = createNavStack([{ page: "home" }]);
  const home = signal(0);
  const detail = signal(0);
  const result = signal("none");
  const prevent = signal(true);
  const requests = signal(0);
  const button = (id: string, text: string, action: () => void) =>
    <View testID={id} onPress={action} style={{ padding: 12, backgroundColor: "#dbeafe", minHeight: 44 }}><Text>{text}</Text></View>;
  return <NavStack state={nav} style={{ flex: 1 }}>
    {route => route.page === "home" ?
      <NavPage title="Home">
        <View testID="route:home" style={{ flex: 1, padding: 16, gap: 12, backgroundColor: "#f8fafc" }}>
          <Text testID={`home-count:${home()}`}>{`Home ${home()}`}</Text>
          {button("home-inc", "Increment home", () => home.set(home() + 1))}
          {button("nav-push", "Push detail", () => nav.push({ page: "detail" }))}
          {button("alert-open", "Open alert", async () => result.set(`alert:${await AlertDialog.show({ title: "Native alert", description: "Choose a result", actions: [{ id: "0", label: "Cancel", role: "cancel" }, { id: "1", label: "Confirm" }] })}`))}
          {button("sheet-open", "Open sheet", async () => result.set(`sheet:${await AlertDialog.show({ title: "Native sheet", presentation: "actionSheet", actions: [{ id: "0", label: "Keep" }, { id: "1", label: "Remove", role: "destructive" }] })}`))}
          <Canvas testID="native-canvas" style={{ width: 280, height: 140, backgroundColor: "#e2e8f0" }} draw={(ctx) => {
            const count = home();
            const g = ctx.createLinearGradient(0, 0, 120, 0);
            g.addColorStop(0, "#ff0000"); g.addColorStop(1, "#0000ff");
            ctx.fillStyle = g; ctx.fillRect(0, 0, 120, 40);
            ctx.save(); ctx.translate(130, 0); ctx.beginPath(); ctx.arc(20, 20, 18, 0, Math.PI * 2);
            ctx.fillStyle = "#00ff00"; ctx.fill(); ctx.restore();
            ctx.beginPath(); ctx.rect(180, 0, 40, 40); ctx.translate(20, 0);
            ctx.fillStyle = "#ff00ff"; ctx.fill(); ctx.setTransform(1, 0, 0, 1, 0, 0);
            ctx.fillStyle = "#202020"; ctx.fillRect(0, 50, 100, 40); ctx.clearRect(20, 60, 20, 20);
            ctx.fillStyle = "#007744"; ctx.font = "bold 18px sans-serif"; ctx.fillText(`Canvas ${count}`, 115, 80);
            ctx.save(); ctx.beginPath(); ctx.rect(0, 100, 50, 30); ctx.clip(); ctx.fillStyle = "#ffaa00";
            ctx.fillRect(0, 95, 100, 45); ctx.restore();
          }} />
          <Text testID={`dialog:${result()}`}>{result()}</Text>
        </View>
      </NavPage> :
      <NavPage title="Detail" popOnBack={!prevent()} onBackPress={() => requests.set(requests() + 1)}>
        <View testID="route:detail" style={{ flex: 1, padding: 16, gap: 12, backgroundColor: "#eef2ff" }}>
          <Text testID={`detail-count:${detail()}`}>{`Detail ${detail()}`}</Text>
          <Text testID={`back-requests:${requests()}`}>{`Back requests ${requests()}`}</Text>
          {button("detail-inc", "Increment detail", () => detail.set(detail() + 1))}
          {button("allow-back", "Allow back", () => prevent.set(false))}
          {button("nav-pop", "Pop detail", () => nav.pop())}
        </View>
      </NavPage>}
  </NavStack>;
}
mount(App);
