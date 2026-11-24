// Writes docs/config-reference.md from schema.json: every field of the configuration, with
// its type and description, so the reference never drifts from the code.

import { readFileSync, writeFileSync } from "node:fs";

const root = new URL("../../", import.meta.url).pathname;
const schema = JSON.parse(readFileSync(root + "schema.json", "utf8"));
const defs = schema.$defs;

/** Type of a field, as shown in the tables */
function type(field) {
  if (field.$ref) return link(field.$ref);
  if (field.anyOf) {
    return field.anyOf
      .filter((t) => t.type !== "null")
      .map(type)
      .join(" or ");
  }
  if (field.oneOf) return field.oneOf.map((v) => `\`"${v.const}"\``).join(" or ");
  if (field.enum) return field.enum.map((v) => `\`"${v}"\``).join(", ");
  const types = [].concat(field.type ?? []).filter((t) => t !== "null");
  if (types.includes("array")) return `list of ${type(field.items ?? {})}`;
  if (types.includes("object")) {
    const values = field.additionalProperties;
    if (values && typeof values === "object") return `table of ${type(values)}`;
    return "table";
  }
  const names = { string: "string", boolean: "boolean", integer: "integer", number: "number" };
  return types.map((t) => names[t] ?? t).join(" or ") || "any";
}

function link(ref) {
  const name = ref.split("/").pop();
  if (name === "Trigger") return "[trigger](#triggers)";
  if (name === "FileAccess") return '`"read"` or `"write"`';
  if (name === "Hook") return "[hook](#hooks)";
  return `[${name.toLowerCase()}](#${name.toLowerCase()})`;
}

function escape(text) {
  return (text ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}

function table(properties, required = [], only) {
  const rows = Object.entries(properties)
    .filter(([name]) => !only || only.includes(name))
    .sort(([a], [b]) => (only ? only.indexOf(a) - only.indexOf(b) : 0))
    .map(([name, field]) => {
      const mark = required.includes(name) ? " (required)" : "";
      return `| \`${name}\`${mark} | ${type(field)} | ${escape(field.description)} |`;
    });
  return ["| Field | Type | Description |", "|-------|------|-------------|", ...rows].join("\n");
}

const hook = defs.Hook.properties;
const groups = [
  ["What it runs", ["name", "trigger", "cmd", "cmds", "steps"]],
  ["Which files", ["include", "exclude", "include_regex", "exclude_regex", "types", "always_run", "stage_fixed"]],
  ["When it runs", ["cache", "branches", "skip_branches", "affected", "timeout"]],
  ["Hooks working together", ["needs", "inputs", "outputs", "reads", "writes"]],
  ["Hooks of other files and repositories", ["path", "from", "rev", "with", "id", "args", "additional_dependencies", "isolate"]],
  ["Permissions", ["permissions"]],
];
const listed = groups.flatMap(([, names]) => names);
const others = Object.keys(hook).filter((name) => !listed.includes(name));
if (others.length) groups.push(["Other fields", others]);

const page = `# Configuration reference

Every field of \`hooker.toml\` (and \`hooker.config.json\`), generated from the [JSON schema](../schema.json). The [configuration guide](configuration.md) explains how they fit together.

## The config file

${table(schema.properties, schema.required, Object.keys(schema.properties).filter((n) => n !== "$schema"))}

## Hooks

Each \`[[hooks]]\` table.

${groups.map(([title, names]) => `### ${title}\n\n${table(hook, defs.Hook.required, names)}`).join("\n\n")}

## Triggers

The Git hook that runs a hook: ${defs.Trigger.enum.map((t) => `\`${t}\``).join(", ")}.

## Step

A step of a hook (\`[[hooks.steps]]\`), see [Hooks graph and variables](graph.md).

${table(defs.Step.properties, defs.Step.required)}

## Permissions

${escape(defs.Permissions.description)}. See [Security](security.md).

${table(defs.Permissions.properties)}

## Variable

A variable of \`[variables]\`, see [Hooks graph and variables](graph.md#variables).

${table(defs.Variable.properties)}

## Param

A parameter of a reusable hooks file (\`[params]\`), see [Shared hooks](shared-hooks.md).

${table(defs.Param.properties)}

## Share

The hooks a repository shares (\`[share]\`), found by \`hooker search\`.

${table(defs.Share.properties, defs.Share.required)}

## Local changes

\`hooker.local.toml\` has the same fields as \`hooker.toml\`, plus \`disabled = true\` on a hook to turn it off. See [Local changes](local-changes.md).
`;

writeFileSync(root + "docs/config-reference.md", page);
