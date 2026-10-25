import { mount, signal, Text, View } from "nsh";
import { Canvas } from "nsh/canvas";
import { Alert, ActionSheet, createNavigator, Screen, Stack } from "nsh/base";

function App() {
  const nav = createNavigator({ page: "home" });
  const home = signal(0);
  const detail = signal(0);
  const result = signal("none");
  const prevent = signal(true);
  const requests = signal(0);
  const button = (id: string, text: string, action: () => void) =>
    <View testID={id} onPress={action} style={{ padding: 12, backgroundColor: "#dbeafe", minHeight: 44 }}><Text>{text}</Text></View>;
  return <Stack navigator={nav} style={{ flex: 1 }}>
    {route => route.page === "home" ?
      <Screen title="Home">
        <View testID="route:home" style={{ flex: 1, padding: 16, gap: 12, backgroundColor: "#f8fafc" }}>
          <Text testID={`home-count:${home()}`}>{`Home ${home()}`}</Text>
          {button("home-inc", "Increment home", () => home.set(home() + 1))}
          {button("nav-push", "Push detail", () => nav.push({ page: "detail" }))}
          {button("alert-open", "Open alert", async () => result.set(`alert:${await Alert.alert("Native alert", "Choose a result", [{ text: "Cancel", style: "cancel" }, { text: "Confirm" }])}`))}
          {button("sheet-open", "Open sheet", async () => result.set(`sheet:${await ActionSheet.show({ title: "Native sheet", options: ["Keep", "Remove"], destructiveButtonIndexes: [1] })}`))}
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
      </Screen> :
      <Screen title="Detail" preventBack={prevent()} onBackRequest={() => requests.set(requests() + 1)}>
        <View testID="route:detail" style={{ flex: 1, padding: 16, gap: 12, backgroundColor: "#eef2ff" }}>
          <Text testID={`detail-count:${detail()}`}>{`Detail ${detail()}`}</Text>
          <Text testID={`back-requests:${requests()}`}>{`Back requests ${requests()}`}</Text>
          {button("detail-inc", "Increment detail", () => detail.set(detail() + 1))}
          {button("allow-back", "Allow back", () => prevent.set(false))}
          {button("nav-pop", "Pop detail", () => nav.pop())}
        </View>
      </Screen>}
  </Stack>;
}
mount(App);
