import { test } from "node:test";
import assert from "node:assert/strict";
import {
  ssim,
  textGeometry,
  compareGeometry,
  imageGeometry,
  withoutFigureText,
} from "../native-pdf-parity.mjs";
test("geometry comparison catches dropped labels, changed wrapping and placement", () => {
  const xml =
    '<html><body><doc><page width="595" height="842"><flow><block><line><word xMin="10" yMin="20" xMax="50" yMax="30">axis π</word></line></block></flow></page></doc></body></html>';
  const reference = textGeometry(xml);
  assert.deepEqual(compareGeometry(reference, textGeometry(xml)), []);
  assert(
    compareGeometry(reference, textGeometry(xml.replace("axis π", "axis □")))
      .length,
  );
  assert(
    compareGeometry(
      reference,
      textGeometry(xml.replace('xMin="10"', 'xMin="11"')),
    ).length,
  );
  assert(
    compareGeometry(
      reference,
      textGeometry(xml.replace("</block>", "<line/></block>")),
    ).length,
  );
  assert(compareGeometry(reference, []).length);
});
test("local-window SSIM detects local content loss even on mostly blank pages", () => {
  const a = Buffer.alloc(64 * 64, 255),
    b = Buffer.from(a);
  assert.equal(ssim(a, b, 64, 64), 1);
  for (let y = 24; y < 32; y++) for (let x = 24; x < 32; x++) b[y * 64 + x] = 0;
  assert(ssim(a, b, 64, 64) < 0.995);
});

test("searchable native figure labels do not masquerade as added body paragraphs", () => {
  const images = imageGeometry(
    '<pdf2xml><page><image left="10" top="20" width="40" height="30"/></page></pdf2xml>',
  );
  const body = {
    width: 100,
    height: 100,
    lines: [[{ text: "body", box: [0, 0, 10, 10] }]],
  };
  const candidate = {
    ...body,
    lines: [...body.lines, [{ text: "axis label", box: [12, 22, 30, 28] }]],
  };
  assert.deepEqual(
    compareGeometry(
      withoutFigureText([body], images),
      withoutFigureText([candidate], images),
    ),
    [],
  );
});
