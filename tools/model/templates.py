#!/usr/bin/env python3
"""Parameterized design templates (occb-model-template-v1).

A template is a complete occb-model-request-v1 whose values come from typed,
range-checked inputs. Placeholders in the request are replaced on
instantiation:

- {"$input": NAME, "as": "number" | "quantity" | "string", "scale": K}
  is the input's value (times K for numbers); "quantity" wraps it as a
  millimeter length for length inputs and a scalar otherwise.
- {"$product": [ITEMS], "as": ...} and {"$sum": [ITEMS], "as": ...}
  combine numbers and number placeholders.

Instantiation fills every input (defaults for those not given), records the
inputs as a family assumption, and returns a request ready for occt_build.
Usage: templates.py list | templates.py instantiate NAME [INPUTS.json]
"""
import copy
import json
import math
import pathlib
import sys

TEMPLATE_DIR = pathlib.Path(__file__).resolve().parent / 'templates'
SCHEMA = 'occb-model-template-v1'
KINDS = ('length', 'number', 'choice')


class TemplateError(ValueError):
    pass


def load_templates(directory=TEMPLATE_DIR):
    templates = {}
    for path in sorted(directory.glob('*.template.json')):
        template = json.loads(path.read_text())
        if template.get('schema') != SCHEMA:
            raise TemplateError(f'{path.name}: expected schema {SCHEMA}')
        name = path.name.removesuffix('.template.json')
        if template.get('id') != name:
            raise TemplateError(f'{path.name}: id must be {name!r}')
        for item in template['inputs']:
            if item.get('kind') not in KINDS:
                raise TemplateError(f'{name}: input {item.get("id")!r} needs a kind of {", ".join(KINDS)}')
        templates[name] = template
    return templates


def summary(template):
    """What a client needs to choose a template and fill its inputs."""
    keys = ('id', 'version', 'title', 'summary', 'use_when', 'not_for', 'inputs', 'checks')
    return {key: template[key] for key in keys if key in template}


def resolve_inputs(template, given):
    if not isinstance(given, dict):
        raise TemplateError('inputs must be an object of input id to value')
    declared = {item['id']: item for item in template['inputs']}
    unknown = sorted(set(given) - set(declared))
    if unknown:
        raise TemplateError(f'unknown input(s) {", ".join(unknown)}; declared: {", ".join(declared)}')
    values = {}
    for name, item in declared.items():
        value = given.get(name, item['default'])
        if item['kind'] == 'choice':
            if value not in item['choices']:
                raise TemplateError(f'{name} must be one of {", ".join(map(str, item["choices"]))}')
        else:
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
                raise TemplateError(f'{name} must be a finite number{" in millimeters" if item["kind"] == "length" else ""}')
            low, high = item.get('minimum'), item.get('maximum')
            if (low is not None and value < low) or (high is not None and value > high):
                raise TemplateError(f'{name} must be in {low}..{high}, got {value}')
            if 'allowed' in item and value not in item['allowed']:
                raise TemplateError(f'{name} must be one of {", ".join(map(str, item["allowed"]))}')
        values[name] = value
    return values, declared


def _value(node, values, declared):
    if '$input' in node:
        name = node['$input']
        if name not in values:
            raise TemplateError(f'placeholder names undeclared input {name!r}')
        value, kind = values[name], declared[name]['kind']
        if isinstance(value, (int, float)):
            value = value * node.get('scale', 1)
    else:
        operator = '$product' if '$product' in node else '$sum'
        items = [_number(item, values, declared) for item in node[operator]]
        value = math.prod(items) if operator == '$product' else math.fsum(items)
        kind = node.get('kind', 'number')
    form = node.get('as', 'number')
    if form == 'quantity':
        if kind == 'length':
            return {'value': value, 'dimension': 'length', 'unit': 'millimeter'}
        return {'value': value, 'dimension': 'scalar', 'unit': None}
    return value


def _number(item, values, declared):
    if isinstance(item, (int, float)) and not isinstance(item, bool):
        return item
    if isinstance(item, dict):
        value = _value({**item, 'as': 'number'}, values, declared)
        if isinstance(value, (int, float)):
            return value
    raise TemplateError(f'arithmetic placeholder needs numbers, got {item!r}')


def _substitute(node, values, declared):
    if isinstance(node, dict):
        if '$input' in node or '$product' in node or '$sum' in node:
            return _value(node, values, declared)
        return {key: _substitute(value, values, declared) for key, value in node.items()}
    if isinstance(node, list):
        return [_substitute(value, values, declared) for value in node]
    return node


def instantiate(template, given=None):
    """A complete build request from `template` and its inputs."""
    values, declared = resolve_inputs(template, given or {})
    request = _substitute(copy.deepcopy(template['request']), values, declared)
    family = request['model']['family']
    family.setdefault('assumptions', []).append({
        'id': 'template-inputs',
        'statement': f'Instantiated from template {template["id"]} v{template["version"]} with inputs '
                     + json.dumps(values, sort_keys=True),
        'provenance': 'template inputs; values are the requester\'s design assumptions',
    })
    return request


def main(argv):
    templates = load_templates()
    if argv[:1] == ['list']:
        print(json.dumps([summary(t) for t in templates.values()], indent=2))
        return 0
    if len(argv) in (2, 3) and argv[0] == 'instantiate' and argv[1] in templates:
        given = json.loads(pathlib.Path(argv[2]).read_text()) if len(argv) == 3 else {}
        try:
            print(json.dumps(instantiate(templates[argv[1]], given), indent=2))
        except TemplateError as error:
            print(f'error: {error}', file=sys.stderr)
            return 1
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
