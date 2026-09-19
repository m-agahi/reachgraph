/*
  The presenter — plan-05 §1, §6.

  IT CLASSIFIES NOTHING. Node category, box membership, edge strength, root
  slug, version grouping, the dispatch class and the unreachable set all arrive
  as fields that the binary computed. There is no browser test harness and
  plan-05 §8.1 says so plainly, so the rule that keeps this file small is the
  rule that keeps the untested surface small.

  ONE DELIBERATE EXCEPTION, named in plan-05 §6.3: compare mode intersects two
  shards' node sets. That is a set operation over two precomputed reachable
  sets, not a traversal and not a reachability computation. It is here rather
  than in Rust because precomputing it would mean emitting O(versions squared)
  files for a view that is usually not opened.

  REPOSITORY TEXT NEVER REACHES THE DOM AS MARKUP. Doc comments, symbol names
  and plugin notes are arbitrary source text; every one of them goes in through
  `textContent` or through Cytoscape's canvas label rendering. A repository
  whose doc comment contains an `onerror` attribute must not run it in a
  reviewer's browser.
*/

(function () {
  "use strict";

  var DATA_ELEMENT = "rg-data";

  /* ---------------------------------------------------------------- utils */

  function el(id) {
    return document.getElementById(id);
  }

  function show(node, visible) {
    if (node) {
      node.hidden = !visible;
    }
  }

  function text(tag, value, className) {
    var node = document.createElement(tag);
    node.textContent = value === null || value === undefined ? "" : String(value);
    if (className) {
      node.className = className;
    }
    return node;
  }

  /* A node's two-part identity as one comparable string.

     Joining two fields is not parsing either of them: neither half is split,
     matched on or given meaning. ADR-0003 field 3 forbids interpreting `raw`,
     and nothing here does. */
  var SEPARATOR = "\u0000";
  function key(plugin, raw) {
    return plugin + SEPARATOR + raw;
  }
  function keyOf(ref) {
    return key(ref.plugin, ref.raw);
  }

  /* Plan-05 §4.3: a null version renders as "unversioned", never as v1. The
     rule extends to the presentation layer, so the substitution happens in
     exactly one place. */
  function versionLabel(version) {
    return version === null || version === undefined ? "unversioned" : version;
  }

  function isUnversioned(version) {
    return version === null || version === undefined;
  }

  /* ------------------------------------------------------------ the model */

  var model = {
    endpoints: null,
    structure: null,
    unreachable: null,
    versions: null,
    shards: {},
    inline: null,
    boxById: {},
    structureByNode: {},
    selection: null,
    compare: null,
    depth: 3,
    strengths: {
      resolved: true,
      "type-inferred": true,
      lexical: true,
      enclosure: true,
      unresolved: true,
    },
    cy: null,
  };

  function readInline() {
    var block = el(DATA_ELEMENT);
    if (!block) {
      return null;
    }
    try {
      return JSON.parse(block.textContent);
    } catch (error) {
      return null;
    }
  }

  function load(path) {
    if (model.inline) {
      if (Object.prototype.hasOwnProperty.call(model.inline, path)) {
        return Promise.resolve(model.inline[path]);
      }
      return Promise.reject(new Error(path + " is not in this page's data"));
    }
    return fetch(path).then(function (response) {
      if (!response.ok) {
        throw new Error(path + ": " + response.status);
      }
      return response.json();
    });
  }

  /* ------------------------------------------------------------ endpoints */

  function renderEndpoints() {
    var host = el("rg-endpoint-list");
    host.textContent = "";

    model.endpoints.operations.forEach(function (operation, index) {
      var group = document.createElement("div");
      group.className = "rg-operation";
      group.setAttribute("data-operation-index", String(index));

      var head = document.createElement("div");
      head.className = "rg-operation-head";
      head.appendChild(
        text(
          "span",
          operation.service + "." + operation.operation,
          "rg-operation-name",
        ),
      );
      head.appendChild(text("span", " " + operation.direction, "rg-badge"));
      /* Plan-05 §4.3.2: the join key is displayed verbatim and compared only
         by equality. It is never split on `/` or `.`, and a `v2` inside it is
         not a version. */
      head.appendChild(text("code", operation.join_key, "rg-operation-key"));
      group.appendChild(head);

      var bound = operation.versions.filter(function (version) {
        return version.binding.state === "bound" && version.shard;
      });

      operation.versions.forEach(function (version) {
        if (version.binding.state !== "bound" || !version.shard) {
          group.appendChild(unboundRow(version));
          return;
        }
        group.appendChild(boundRow(operation, version));
      });

      /* Plan-05 §6.3: Compare is offered only WITHIN one operation group, and
         only when that group has two or more bound versions. There is no "all
         versions" entry anywhere — a union with no per-node attribution is
         what ADR-0007 forbids, and naming it "All" does not make it
         explicit. */
      if (bound.length >= 2) {
        group.appendChild(compareRow(operation, bound));
      }

      host.appendChild(group);
    });
  }

  function boundRow(operation, version) {
    var row = document.createElement("button");
    row.type = "button";
    row.className = "rg-version-row";
    row.setAttribute("aria-pressed", "false");
    row.setAttribute("data-shard", version.shard);

    var badge = text("span", versionLabel(version.version), "rg-badge");
    if (isUnversioned(version.version)) {
      badge.className = "rg-badge rg-badge-unversioned";
    }
    row.appendChild(badge);
    row.appendChild(
      text("span", version.node_count + " nodes", "rg-version-count"),
    );
    if (version.frontier_count > 0) {
      row.appendChild(
        text("span", version.frontier_count + " frontier", "rg-badge"),
      );
    }

    row.addEventListener("click", function () {
      selectShard(operation, version);
    });
    return row;
  }

  /* Plan-05 §4.3: an unbound version is rendered as a non-selectable row
     carrying its reason, styled as a reported gap — not hidden and not greyed
     into invisibility. */
  function unboundRow(version) {
    var row = document.createElement("div");
    row.className = "rg-version-gap";
    row.setAttribute("data-binding", "unbound");

    var badge = text("span", versionLabel(version.version), "rg-badge");
    if (isUnversioned(version.version)) {
      badge.className = "rg-badge rg-badge-unversioned";
    }
    row.appendChild(badge);
    row.appendChild(text("span", " no handler bound", "rg-badge"));
    row.appendChild(
      text(
        "p",
        version.binding.reason || "the provider gave no reason",
        "rg-gap-reason",
      ),
    );
    return row;
  }

  function compareRow(operation, bound) {
    var row = document.createElement("button");
    row.type = "button";
    row.className = "rg-version-row";
    row.setAttribute("data-compare", "1");
    row.setAttribute("aria-pressed", "false");
    row.appendChild(text("span", "Compare", "rg-badge"));
    row.appendChild(
      text(
        "span",
        bound
          .map(function (version) {
            return versionLabel(version.version);
          })
          .join(" / "),
        "rg-version-count",
      ),
    );
    row.addEventListener("click", function () {
      startCompare(operation, bound);
    });
    return row;
  }

  function pressOnly(node) {
    Array.prototype.forEach.call(
      document.querySelectorAll(".rg-version-row"),
      function (other) {
        other.setAttribute("aria-pressed", other === node ? "true" : "false");
      },
    );
  }

  /* ---------------------------------------------------------- the version
     toggle over the selected group */

  function renderVersionToggle(operation, activeShard, comparing) {
    var host = el("rg-version-toggle");
    host.textContent = "";
    show(el("rg-version-control"), true);

    operation.versions.forEach(function (version) {
      var button = document.createElement("button");
      button.type = "button";
      button.textContent = versionLabel(version.version);
      var selectable = version.binding.state === "bound" && !!version.shard;
      button.disabled = !selectable;
      button.setAttribute(
        "aria-pressed",
        !comparing && selectable && version.shard === activeShard
          ? "true"
          : "false",
      );
      if (!selectable) {
        button.title = version.binding.reason || "";
        button.setAttribute("data-binding", "unbound");
      }
      button.addEventListener("click", function () {
        selectShard(operation, version);
      });
      host.appendChild(button);
    });

    var bound = operation.versions.filter(function (version) {
      return version.binding.state === "bound" && version.shard;
    });
    if (bound.length >= 2) {
      var compare = document.createElement("button");
      compare.type = "button";
      compare.textContent = "Compare";
      compare.setAttribute("data-compare", "1");
      compare.setAttribute("aria-pressed", comparing ? "true" : "false");
      compare.addEventListener("click", function () {
        startCompare(operation, bound);
      });
      host.appendChild(compare);
    }
  }

  /* --------------------------------------------------------------- shards */

  function shard(path) {
    if (model.shards[path]) {
      return Promise.resolve(model.shards[path]);
    }
    return load(path).then(function (document_) {
      model.shards[path] = document_;
      return document_;
    });
  }

  function selectShard(operation, version) {
    model.compare = null;
    show(el("rg-compare-legend"), false);
    shard(version.shard)
      .then(function (document_) {
        model.selection = {
          operation: operation,
          version: version,
          shard: document_,
        };
        pressOnly(
          document.querySelector(
            '.rg-version-row[data-shard="' + cssEscape(version.shard) + '"]',
          ),
        );
        renderVersionToggle(operation, version.shard, false);
        renderShardNote(document_);
        renderDepthControl(document_);
        draw();
      })
      .catch(failed);
  }

  function cssEscape(value) {
    if (window.CSS && window.CSS.escape) {
      return window.CSS.escape(value);
    }
    return value.replace(/["\\]/g, "\\$&");
  }

  /* Plan-05 §4.4: the shard's own depth limit is STATED, never silently
     presented as the whole reachable set. */
  function renderShardNote(document_) {
    var note = el("rg-shard-note");
    note.textContent = "";
    var frontier = document_.frontier ? document_.frontier.length : 0;

    if (document_.depth_limit === null || document_.depth_limit === undefined) {
      show(note, false);
      return;
    }

    note.appendChild(
      text(
        "span",
        "The walk stopped at depth " +
          document_.depth_limit +
          ". " +
          (frontier === 0
            ? "No node sits at that boundary with out-edges that were not followed."
            : frontier +
              " node(s) sit at that boundary: their out-edges were not followed, so they are drawn as frontier rather than as leaves."),
      ),
    );
    show(note, true);
  }

  function renderDepthControl(document_) {
    var control = el("rg-depth-control");
    var slider = el("rg-depth");
    var maximum = 0;
    var anyDepth = false;

    document_.nodes.forEach(function (node) {
      if (node.depth !== null && node.depth !== undefined) {
        anyDepth = true;
        if (node.depth > maximum) {
          maximum = node.depth;
        }
      }
    });

    /* Plan-05 §6.2: in the index-wide view `depth` is null for every node and
       the control has nothing to mean, so it is ABSENT rather than zeroed. */
    if (!anyDepth) {
      show(control, false);
      return;
    }

    slider.max = String(maximum);
    var start = Math.min(3, maximum);
    slider.value = String(start);
    model.depth = start;
    el("rg-depth-value").textContent = String(start);
    show(control, true);
  }

  /* ------------------------------------------------------------- drawing */

  function structureOf(ref) {
    return model.structureByNode[keyOf(ref)] || null;
  }

  function nodeClasses(node, structure) {
    var classes = [];
    classes.push(node.symbol ? "indexed" : "unindexed");
    if (node.frontier) {
      classes.push("frontier");
    }
    if (node.symbol && node.symbol.is_test) {
      classes.push("test");
    }
    classes.push("category-" + (node.category || "unclassified"));
    if (structure && structure.dispatch) {
      classes.push(structure.dispatch);
    }
    return classes;
  }

  /* Plan-05 §4.4.3: an unindexed node has no name, only an opaque id. This
     renders that string VERBATIM and never splits it — no `/` or `:` in it is
     given meaning, not to shorten the label, not to guess a module. The node
     is visibly marked unindexed so a reader does not mistake the id for a
     name. Honest and ugly, and recorded as such in plan-05 §9.3. */
  function labelOf(node) {
    return node.symbol ? node.symbol.name : node.id.raw;
  }

  function elementsOf(document_) {
    var elements = [];
    var seenBoxes = {};
    var present = {};

    document_.nodes.forEach(function (node) {
      present[keyOf(node.id)] = true;
    });

    document_.nodes.forEach(function (node) {
      var structure = structureOf(node.id);
      var parent = structure && structure.box ? structure.box : null;
      var chain = parent;
      while (chain && !seenBoxes[chain]) {
        var box = model.boxById[chain];
        if (!box) {
          break;
        }
        seenBoxes[chain] = true;
        elements.push({
          data: {
            id: chain,
            label: box.label,
            parent: box.parent || undefined,
            rgKind: box.kind,
          },
          classes: "rg-box rg-box-" + box.kind,
        });
        chain = box.parent;
      }

      elements.push({
        data: {
          id: keyOf(node.id),
          label: labelOf(node),
          parent: parent || undefined,
          rgDepth: node.depth,
        },
        classes: nodeClasses(node, structure).join(" "),
      });
    });

    document_.edges.forEach(function (edge, index) {
      var from = keyOf(edge.from);
      var strength =
        edge.to.state === "unresolved" ? "unresolved" : edge.inference_mode;
      var target;

      if (edge.to.state === "resolved") {
        target = keyOf(edge.to.node);
        if (!present[target]) {
          return;
        }
      } else {
        /* Plan-05 §4.4: an unresolved target survives onto the screen as a
           stub. It is never dropped and never collapsed to its first
           candidate — design.md §8: show a missing edge as missing, never
           infer one to fill a hole. */
        target = "rg-unresolved-" + index;
        elements.push({
          data: { id: target, label: edge.to.name + " — not resolved" },
          classes: "unresolved-stub",
        });
      }

      elements.push({
        data: {
          id: "rg-edge-" + index,
          source: from,
          target: target,
          rgStrength: strength,
          rgIndex: index,
        },
        classes: "strength-" + strength,
      });
    });

    return elements;
  }

  function style() {
    return [
      {
        selector: "node",
        style: {
          label: "data(label)",
          "font-size": 9,
          "text-valign": "center",
          "text-wrap": "ellipsis",
          "text-max-width": 130,
          shape: "round-rectangle",
          width: 130,
          height: 26,
          "background-color": "#ffffff",
          "border-width": 2,
          "border-color": "#16191d",
          color: "#16191d",
        },
      },
      {
        selector: "node.unindexed",
        style: { "border-style": "dashed", "border-color": "#5a6470" },
      },
      {
        selector: "node.unresolved-stub",
        style: {
          "border-style": "dotted",
          "border-color": "#b26a00",
          shape: "diamond",
        },
      },
      /* Plan-05 §4.4.2: a frontier node has out-edges that were NOT followed.
         Drawing it as a leaf would be a false claim, so it carries a distinct
         border at every slider position. */
      {
        selector: "node.frontier",
        style: {
          "border-style": "double",
          "border-width": 6,
          "border-color": "#b26a00",
        },
      },
      /* ADR-0729. A call through a generic resolves to the trait's
         declaration, so this node is not the code that runs. */
      {
        selector: "node.trait-declaration",
        style: {
          "border-color": "#6b4fbb",
          "border-style": "dashed",
          "border-width": 4,
          shape: "hexagon",
          width: 140,
        },
      },
      {
        selector: "node.rg-box",
        style: {
          label: "data(label)",
          "text-valign": "top",
          "text-halign": "center",
          "font-size": 10,
          shape: "round-rectangle",
          "background-opacity": 0.06,
          "background-color": "#16191d",
          "border-width": 1,
          "border-color": "#5a6470",
          "border-style": "solid",
          padding: 14,
          width: "label",
          height: "label",
        },
      },
      {
        selector: "node.rg-box-unit",
        style: { "border-width": 2, "border-style": "solid" },
      },
      {
        selector: "node.rg-box-type",
        style: { "border-style": "dashed" },
      },
      {
        selector: "edge",
        style: {
          width: 3,
          "line-color": "#16191d",
          "target-arrow-color": "#16191d",
          "target-arrow-shape": "triangle",
          "curve-style": "bezier",
          opacity: 1,
        },
      },
      /* Plan-05 §5. Dash pattern and weight carry the signal; colour is
         redundant reinforcement, because artifacts are printed and
         screenshotted into greyscale. */
      {
        selector: "edge.strength-type-inferred",
        style: { "line-style": "dashed", "line-dash-pattern": [10, 4], width: 2.5, opacity: 0.9 },
      },
      {
        selector: "edge.strength-lexical",
        style: { "line-style": "dashed", "line-dash-pattern": [4, 4], width: 2, opacity: 0.75 },
      },
      {
        selector: "edge.strength-enclosure",
        style: { "line-style": "dotted", "line-dash-pattern": [1, 4], width: 1.5, opacity: 0.6 },
      },
      {
        selector: "edge.strength-unresolved",
        style: { "line-style": "dotted", "line-dash-pattern": [1, 4], width: 1.5, opacity: 0.6 },
      },
      /* Compare mode — plan-05 §6.3. Border, fill and a text badge, three
         redundant channels. */
      {
        selector: "node.compare-first-only",
        style: { "border-style": "solid", "background-color": "#e9edf2" },
      },
      {
        selector: "node.compare-second-only",
        style: { "border-style": "solid", "background-color": "#f5eee2" },
      },
      {
        selector: "node.compare-both",
        style: { "border-style": "double", "border-width": 6 },
      },
      { selector: ".rg-hidden", style: { display: "none" } },
    ];
  }

  function layout() {
    var name = window.cytoscape && cytoscapeHasFcose() ? "fcose" : "cose";
    return {
      name: name,
      animate: false,
      nodeDimensionsIncludeLabels: true,
      randomize: true,
      fit: true,
      padding: 20,
    };
  }

  function cytoscapeHasFcose() {
    try {
      return !!window.cytoscape("layout", "fcose");
    } catch (error) {
      return false;
    }
  }

  function draw() {
    var document_ = model.selection && model.selection.shard;
    if (!document_) {
      return;
    }
    show(el("rg-empty"), false);

    if (model.cy) {
      model.cy.destroy();
    }
    model.cy = window.cytoscape({
      container: el("rg-cy"),
      elements: elementsOf(document_),
      style: style(),
      layout: layout(),
      wheelSensitivity: 0.2,
    });

    model.cy.on("tap", "node", function (event) {
      inspect(document_, event.target);
    });

    if (model.compare) {
      paintCompare();
    }
    applyFilters();
  }

  /* --------------------------------------------------------------- filters */

  function applyFilters() {
    if (!model.cy) {
      return;
    }
    model.cy.batch(function () {
      model.cy.nodes().forEach(function (node) {
        if (node.hasClass("rg-box")) {
          return;
        }
        var depth = node.data("rgDepth");
        var tooDeep =
          depth !== null && depth !== undefined && depth > model.depth;
        /* Plan-05 §6.2: a frontier marker stays visible at every slider
           position. Hiding a node above the threshold is a display choice the
           reader made; drawing a frontier node as a leaf is a false claim
           about the data, and the two must not be confused. */
        node.toggleClass("rg-hidden", tooDeep && !node.hasClass("frontier"));
      });
      model.cy.edges().forEach(function (edge) {
        var strength = edge.data("rgStrength");
        edge.toggleClass("rg-hidden", !model.strengths[strength]);
      });
    });
  }

  function renderStrengthFilter() {
    var host = el("rg-strength-filter");
    host.textContent = "";
    Object.keys(model.strengths).forEach(function (strength) {
      var label = document.createElement("label");
      var box = document.createElement("input");
      box.type = "checkbox";
      box.checked = model.strengths[strength];
      box.setAttribute("data-strength", strength);
      box.addEventListener("change", function () {
        model.strengths[strength] = box.checked;
        applyFilters();
      });
      label.appendChild(box);
      label.appendChild(document.createTextNode(" " + strength));
      host.appendChild(label);
    });
  }

  /* ------------------------------------------------------------- inspector */

  function inspect(document_, target) {
    var panel = el("rg-inspector");
    var body = el("rg-inspector-body");
    body.textContent = "";

    if (target.hasClass("rg-box")) {
      el("rg-inspector-title").textContent = target.data("label");
      addRow(body, "box kind", target.data("rgKind"));
      show(panel, true);
      return;
    }

    var id = target.id();
    var node = null;
    document_.nodes.forEach(function (candidate) {
      if (keyOf(candidate.id) === id) {
        node = candidate;
      }
    });
    if (!node) {
      el("rg-inspector-title").textContent = target.data("label");
      show(panel, true);
      return;
    }

    el("rg-inspector-title").textContent = labelOf(node);
    addRow(body, "id", node.id.raw);
    addRow(body, "plugin", node.id.plugin);

    if (!node.symbol) {
      /* Plan-05 §4.4.1: no provider emitted a symbol for this id. It is drawn
         and never hidden — deleting it would delete the cross-repository seam,
         which is the thesis. */
      addRow(
        body,
        "not indexed",
        "An edge resolved to this id and no provider emitted a symbol for it, so it has no name and the label above is the opaque id itself.",
      );
    } else {
      addRow(body, "kind", node.symbol.kind + " (" + node.symbol.raw_kind + ")");
      addRow(body, "file", String(node.symbol.range.file));
      if (node.symbol.range.span) {
        addRow(
          body,
          "offsets " + positionEncoding(document_),
          node.symbol.range.span.start + ".." + node.symbol.range.span.end,
        );
      } else {
        /* Plan-05 §4.4.1: the plugin knows the file, not the offset, and says
           so. The file survives; only the jump degrades. */
        addRow(
          body,
          "offsets",
          "the plugin reported no offset within this file",
        );
      }
      if (node.symbol.doc) {
        addRow(body, "doc", node.symbol.doc.split("\n")[0]);
      }
      addRow(body, "test code", node.symbol.is_test ? "yes" : "no");
    }

    addRow(body, "category", node.category || "unclassified");
    addRow(body, "unit", node.unit || "none — this node belongs to no unit");

    if (node.frontier) {
      addRow(
        body,
        "frontier",
        "The walk stopped here at this shard's depth limit. Its out-edges were not followed; this is not a leaf.",
      );
    }

    var structure = structureOf(node.id);
    if (structure && structure.container_raw_kind) {
      addRow(body, "enclosed by", structure.container_raw_kind);
    }
    if (structure && structure.dispatch === "trait-declaration") {
      addRow(body, "trait declaration", model.structure.trait_declaration_note);
    }

    show(panel, true);
  }

  function positionEncoding(document_) {
    /* Plan-05 §4.3.1: a span is meaningless without the unit it counts in. */
    if (!document_.plugins || document_.plugins.length === 0) {
      return "";
    }
    return "(" + document_.plugins[0].position_encoding + ")";
  }

  function addRow(body, term, value) {
    body.appendChild(text("dt", term));
    body.appendChild(text("dd", value, "rg-mono"));
  }

  /* --------------------------------------------------------------- compare */

  /* THE ONE EXCEPTION to the thin-presenter rule (plan-05 §6.3): a set
     intersection of two precomputed reachable sets. Not a traversal, and not a
     reachability computation. */
  function startCompare(operation, bound) {
    var first = bound[0];
    var second = bound[1];

    Promise.all([shard(first.shard), shard(second.shard)])
      .then(function (both) {
        var firstSet = {};
        both[0].nodes.forEach(function (node) {
          firstSet[keyOf(node.id)] = true;
        });
        var secondSet = {};
        both[1].nodes.forEach(function (node) {
          secondSet[keyOf(node.id)] = true;
        });

        var merged = { nodes: [], edges: [], plugins: both[0].plugins, depth_limit: both[0].depth_limit, frontier: [] };
        var seen = {};
        [both[0], both[1]].forEach(function (document_) {
          document_.nodes.forEach(function (node) {
            var id = keyOf(node.id);
            if (!seen[id]) {
              seen[id] = true;
              merged.nodes.push(node);
            }
          });
          document_.edges.forEach(function (edge) {
            merged.edges.push(edge);
          });
          document_.frontier.forEach(function (ref) {
            merged.frontier.push(ref);
          });
        });

        model.selection = { operation: operation, version: null, shard: merged };
        model.compare = {
          first: first,
          second: second,
          firstSet: firstSet,
          secondSet: secondSet,
        };

        el("rg-compare-names").textContent =
          versionLabel(first.version) + " and " + versionLabel(second.version);
        el("rg-compare-first-badge").textContent = versionLabel(first.version);
        el("rg-compare-second-badge").textContent = versionLabel(second.version);
        show(el("rg-compare-legend"), true);
        pressOnly(document.querySelector('.rg-version-row[data-compare="1"]'));
        renderVersionToggle(operation, null, true);
        renderShardNote(merged);
        renderDepthControl(merged);
        draw();
      })
      .catch(failed);
  }

  function paintCompare() {
    model.cy.nodes().forEach(function (node) {
      if (node.hasClass("rg-box")) {
        return;
      }
      var id = node.id();
      var inFirst = !!model.compare.firstSet[id];
      var inSecond = !!model.compare.secondSet[id];
      var badge = node.data("label");
      if (inFirst && inSecond) {
        node.addClass("compare-both");
        badge = badge + "  [" + versionLabel(model.compare.first.version) + " " + versionLabel(model.compare.second.version) + "]";
      } else if (inFirst) {
        node.addClass("compare-first-only");
        badge = badge + "  [" + versionLabel(model.compare.first.version) + "]";
      } else if (inSecond) {
        node.addClass("compare-second-only");
        badge = badge + "  [" + versionLabel(model.compare.second.version) + "]";
      }
      node.data("label", badge);
    });
  }

  /* ----------------------------------------------------------- the panels */

  function renderUnreachable() {
    var button = el("rg-unreachable-load");
    var table = el("rg-unreachable-table");
    var rows = el("rg-unreachable-rows");
    var includeTests = el("rg-unreachable-tests");

    function fill() {
      rows.textContent = "";
      model.unreachable.nodes.forEach(function (node) {
        if (!includeTests.checked && node.is_test) {
          return;
        }
        var row = document.createElement("tr");
        row.appendChild(text("td", node.name));
        row.appendChild(text("td", String(node.file), "rg-mono"));
        /* Plan-05 §4.4.1: a null category is its own bucket, never folded
           into third-party. */
        row.appendChild(text("td", node.category || "unclassified"));
        row.appendChild(text("td", node.is_test ? "yes" : ""));
        row.appendChild(
          text(
            "td",
            node.possibly_reachable_via_unresolved
              ? "an unresolved call may reach this"
              : "",
          ),
        );
        rows.appendChild(row);
      });
    }

    button.addEventListener("click", function () {
      load("unreachable.json")
        .then(function (document_) {
          model.unreachable = document_;
          fill();
          show(table, true);
          button.disabled = true;
        })
        .catch(failed);
    });

    includeTests.addEventListener("change", function () {
      if (model.unreachable) {
        fill();
      }
    });
  }

  function renderSunset() {
    var hint = el("rg-sunset-hint");
    var body = el("rg-sunset-body");
    var summary = model.versions.summary_by_contract || {};
    var contracts = Object.keys(summary);

    if (contracts.length === 0) {
      /* Says it has nothing rather than showing an empty table. A contract
         with one version has no sunset question, and inventing a second
         version to fill the table is exactly what ADR-0007 forbids. */
      hint.textContent =
        "No contract in this index carries exactly two versions, so there is no sunset comparison to make. This index covers " +
        model.versions.version_keys.length +
        " version key(s).";
      return;
    }

    hint.textContent =
      "Per contract with exactly two versions: what each version alone reaches, and what they share.";

    var table = document.createElement("table");
    var head = document.createElement("tr");
    ["contract", "first only", "second only", "both"].forEach(function (name) {
      head.appendChild(text("th", name));
    });
    table.appendChild(head);

    contracts.forEach(function (contract) {
      var counts = summary[contract];
      var row = document.createElement("tr");
      row.appendChild(text("td", contract, "rg-mono"));
      row.appendChild(text("td", counts.v1_only));
      row.appendChild(text("td", counts.v2_only));
      row.appendChild(text("td", counts.both));
      table.appendChild(row);
    });
    body.appendChild(table);
  }

  /* ------------------------------------------------------------- start-up */

  function failed(error) {
    el("rg-load-error-detail").textContent = String(error && error.message ? error.message : error);
    show(el("rg-load-error"), true);
  }

  function indexStructure() {
    model.structure.boxes.forEach(function (box) {
      model.boxById[box.id] = box;
    });
    model.structure.nodes.forEach(function (node) {
      model.structureByNode[key(node.plugin, node.raw)] = node;
    });
  }

  function start() {
    model.inline = readInline();

    /* Plan-05 §6.1: `fetch()` against `file://` is CORS-blocked. With no
       inline data the page says what to do instead of failing silently. */
    if (!model.inline && window.location.protocol === "file:") {
      show(el("rg-file-protocol"), true);
      return;
    }

    Promise.all([
      load("endpoints.json"),
      load("structure.json"),
      load("versions.json"),
    ])
      .then(function (documents) {
        model.endpoints = documents[0];
        model.structure = documents[1];
        model.versions = documents[2];
        indexStructure();

        show(el("rg-main"), true);
        renderEndpoints();
        renderStrengthFilter();
        renderUnreachable();
        renderSunset();

        el("rg-depth").addEventListener("input", function (event) {
          model.depth = Number(event.target.value);
          el("rg-depth-value").textContent = String(model.depth);
          applyFilters();
        });
        el("rg-inspector-close").addEventListener("click", function () {
          show(el("rg-inspector"), false);
        });
      })
      .catch(failed);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
