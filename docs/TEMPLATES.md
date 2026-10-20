# Design templates

A template is a parameterized starting design: a complete build request
whose dimensions, materials, loads and limits come from typed inputs. An AI
client (or a person) picks the template that fits a functional need, fills
the inputs it knows, and gets a request that builds and verifies like any
other model, and that can then be edited freely. Templates live in
[`tools/model/templates/`](../tools/model/templates) as
`occb-model-template-v1` files.

| Template | For | Checks |
|---|---|---|
| `l-bracket` | Mounting one part square to another with bolts, loaded at the top of the upright | valid solid, exact minimum wall, mass, bending at the upright's root (load × height) |
| `printed-enclosure` | An open-top box sized from its inside, printed or machined | valid solid, exact minimum wall, fits the print bed, heat-deflection margin in service |
| `shaft` | Pins, axles and spindles on two supports with a mid-span load | valid solid, mass, bending at mid-span (load × length / 4) |
| `sheet-metal-channel` | A U-channel bent from one sheet, with its flat blank | valid folded and flat solids, blank fits the stock, mass |

Each template says what it is `not_for` (for example, lids or keyways) so a
client knows when to start elsewhere or add features afterwards.

## Inputs

Inputs have a `kind`: `length` (millimeters), `number`, or `choice`, with a
`default`, a `description`, and `minimum`/`maximum` or `choices`; a length
may also list `allowed` values, such as catalog bolt sizes. Material inputs
offer only catalog grades whose sources print the properties the template's
checks need ([Materials](MATERIALS.md)). Omitted inputs take their defaults;
unknown, out-of-range or wrongly typed inputs are refused before anything is
built.

Geometric inputs become family parameters (defaults filled in, ranges kept),
so the instantiated model stays parametric. Loads, limits, materials and
derived values (such as a bending moment from a load and a span) are written
into the requirements. The inputs used are recorded as the family assumption
`template-inputs`.

## Using them

Over MCP: `occt_list_templates` returns each template's purpose, checks and
inputs; `occt_instantiate_template` with `template` and `inputs` returns the
request for `occt_build`. From a shell:

```sh
python3 tools/model/templates.py list
python3 tools/model/templates.py instantiate shaft inputs.json > shaft.request.json
```

## Format

A template file holds `id`, `version`, `title`, `summary`, `use_when`,
`not_for`, `checks`, `inputs` and `request`. In `request`, placeholders are
replaced on instantiation:

- `{"$input": NAME, "as": "number" | "quantity" | "string", "scale": K}`: the
  input's value, times `K` for numbers. `"quantity"` writes a millimeter length
  for length inputs and a scalar otherwise.
- `{"$product": [ITEMS]}` and `{"$sum": [ITEMS]}`: numbers and number
  placeholders combined. Add `"kind": "length", "as": "quantity"` to write the
  result as a length.

Tests instantiate every template with its defaults, validate the result
against the generated request schema, build it with every requirement
passing, check that inputs change the design (50 kN on the default shaft is
rejected at its mid-span bending check, and a 60 mm shaft passes), and check
that bad inputs are refused.
