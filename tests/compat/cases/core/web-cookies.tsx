import { Text, View, WebView, signal, type WebViewRef } from "nsh";

export default function App() {
  const result = signal("Ready");
  let page!: WebViewRef;
  return <View style={{ flex: 1, paddingTop: 64 }}>
    <View testID="cookies" onPress={async () => {
      const failures: string[] = [];
      for (const call of [
        () => page.cookieStore.getAll(null as unknown as { url: string }),
        () => page.cookieStore.getAll({ url: "javascript:alert(1)" }),
        () => page.cookieStore.get({ url: "https://example.com/", timeout: -1 }),
      ]) {
        try { await call(); failures.push("resolved"); }
        catch (error) { failures.push((error as Error).name); }
      }
      const codes: string[] = [];
      for (const call of [
        () => page.cookieStore.getAll({ url: "https://example.com/", timeout: 1000 }),
        () => page.cookieStore.get({ url: "https://example.com/", name: "sid", timeout: 1000 }),
      ]) {
        try { await call(); codes.push("resolved"); }
        catch (error) { codes.push((error as Error & { code: string }).code); }
      }
      result.set(`Cookies: ${failures.join(",")}/${codes.join(",")}`);
    }}><Text>Cookies</Text></View>
    <Text>{result()}</Text>
    <WebView ref={page} html="<title>Compat Cookies</title>" style={{ width: 200, height: 120 }} />
  </View>;
}
