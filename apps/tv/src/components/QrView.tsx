import { View } from "react-native";
import { qrMatrix } from "../qr";

/** QR drawn from Views: no SVG or canvas dependency, so it renders on Vega. */
export function QrView({ text, size = 320 }: { text: string; size?: number }) {
  const rows = qrMatrix(text);
  const cell = size / rows.length;
  return (
    <View testID="qr" style={{ width: size, height: size, backgroundColor: "#fff", padding: 0 }}>
      {rows.map((row, y) => (
        <View key={y} style={{ flexDirection: "row", height: cell }}>
          {row.map((on, x) => (
            <View key={x} style={{ width: cell, height: cell, backgroundColor: on ? "#000" : "#fff" }} />
          ))}
        </View>
      ))}
    </View>
  );
}
