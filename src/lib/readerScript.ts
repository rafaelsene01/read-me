// SPEC: read-aloud (TTS-03, TTS-12, TTS-17, TTS-18, TTS-19), epub-fidelity (FID-14)

/**
 * The only script that runs inside the book's iframe.
 *
 * It exists because the iframe stopped being `sandbox=""` and became
 * `sandbox="allow-scripts"` — the sentence being read has to be marked on the
 * page itself, and nothing outside the frame can reach into it. The book's own
 * JavaScript is removed at extraction (`reader/sanitize.rs`), so what runs here
 * is this file and nothing else.
 *
 * `allow-same-origin` is deliberately still absent: the frame keeps an opaque
 * origin, so this code cannot see the app, cannot call `invoke`, and cannot
 * touch its storage. The only channel is `postMessage`, and it carries two
 * message shapes and no more.
 *
 * Marking uses the **CSS Custom Highlight API**, which paints a `Range` without
 * inserting a single element. That matters more than it looks: wrapping the
 * sentence in a `<span>` would edit the book's own DOM, and a book whose CSS
 * targets `p > em:first-child` would visibly change while being read. When the
 * API is missing the code falls back to a wrapper span, and the fallback is the
 * degraded path, not the default.
 */
export const READER_SCRIPT = String.raw`
(function () {
  var HL = "readaloud";
  var blocks = function () { return Array.prototype.slice.call(document.body.children); };
  var fallback = null;

  // The text of a node tree as the app sees it: the same collapse
  // \`html::visible_text\` does in Rust, so an offset means the same thing on
  // both sides of the boundary.
  function normalize(s) { return s.replace(/\s+/g, " "); }

  // Walks the text nodes of a block, returning [node, startOffsetInBlock] pairs
  // plus the block's normalized text.
  function textMap(block) {
    var walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT, null);
    var nodes = [], text = "", node;
    while ((node = walker.nextNode())) {
      var piece = normalize(node.nodeValue);
      if (!piece) continue;
      // Collapse the seam too: two text nodes separated by a tag are one space
      // apart in the normalized view, never zero.
      if (text && !/\s$/.test(text) && !/^\s/.test(piece)) { text += " "; }
      nodes.push({ node: node, at: text.length, len: piece.length });
      text += piece;
    }
    return { nodes: nodes, text: text };
  }

  function rangeFor(block, from, to) {
    var map = textMap(block);
    var range = document.createRange();
    var started = false;
    for (var i = 0; i < map.nodes.length; i++) {
      var n = map.nodes[i];
      if (!started && from < n.at + n.len) {
        range.setStart(n.node, Math.max(0, Math.min(from - n.at, n.node.nodeValue.length)));
        started = true;
      }
      if (started && to <= n.at + n.len) {
        range.setEnd(n.node, Math.max(0, Math.min(to - n.at, n.node.nodeValue.length)));
        return range;
      }
    }
    return started ? range : null;
  }

  function clearMark() {
    if (window.CSS && CSS.highlights) { CSS.highlights.delete(HL); }
    if (fallback && fallback.parentNode) {
      var parent = fallback.parentNode;
      while (fallback.firstChild) { parent.insertBefore(fallback.firstChild, fallback); }
      parent.removeChild(fallback);
      parent.normalize();
    }
    fallback = null;
  }

  function mark(blockIndex, from, to) {
    clearMark();
    var block = blocks()[blockIndex];
    if (!block) return;
    var range = rangeFor(block, from, to);
    if (!range) return;
    if (window.CSS && CSS.highlights && window.Highlight) {
      CSS.highlights.set(HL, new Highlight(range));
    } else {
      // Degraded path: this edits the book's DOM, so it only runs where the
      // highlight API is absent.
      try {
        fallback = document.createElement("span");
        fallback.className = "readaloud-mark";
        range.surroundContents(fallback);
      } catch (e) { fallback = null; }
    }
    if (block.scrollIntoView) {
      block.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }

  // A click anywhere in the text answers "which block, and how far into it",
  // and the app turns that into a sentence. Doing the mapping here and the
  // sentence lookup there keeps one definition of a sentence, in Rust.
  document.addEventListener("click", function (event) {
    var all = blocks();
    for (var i = 0; i < all.length; i++) {
      if (!all[i].contains(event.target)) continue;
      var offset = 0;
      var caret = document.caretPositionFromPoint
        ? document.caretPositionFromPoint(event.clientX, event.clientY)
        : null;
      if (caret) {
        var map = textMap(all[i]);
        for (var j = 0; j < map.nodes.length; j++) {
          if (map.nodes[j].node === caret.offsetNode) {
            offset = map.nodes[j].at + caret.offset;
            break;
          }
        }
      }
      parent.postMessage({ readaloud: "click", block: i, offset: offset }, "*");
      return;
    }
  });

  window.addEventListener("message", function (event) {
    var data = event.data || {};
    if (data.readaloud === "mark") { mark(data.block, data.from, data.to); }
    else if (data.readaloud === "clear") { clearMark(); }
  });

  parent.postMessage({ readaloud: "ready" }, "*");
})();
`;

/** Painted by the highlight API; the class is the fallback's twin. The text on
 *  the mark is black, not inherited: with a dark theme forced on the page
 *  (FID-14) inherited text is white, and white on this yellow is unreadable. */
export const READER_SCRIPT_CSS = String.raw`
::highlight(readaloud){background:#ffe58a;color:#000}
.readaloud-mark{background:#ffe58a;color:#000}
`;
