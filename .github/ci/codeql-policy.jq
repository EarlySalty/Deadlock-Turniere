# Invoke with jq --slurp --exit-status: more than one JSON document is invalid.
# CodeQL stores query-pack rules in tool.extensions, not necessarily in driver.
# Reporting uploads are unrelated to this mandatory, fail-closed decision.
def nonempty_string: type == "string" and length > 0;
def rules_valid:
  type == "array" and
  all(.[]; type == "object" and (.id | nonempty_string));
def component_valid:
  type == "object" and (.name | nonempty_string) and
  (if has("rules") then (.rules | rules_valid) else true end);
def notifications_valid:
  type == "array" and all(.[];
    type == "object" and
    (.level == "none" or .level == "note" or .level == "warning"));
def invocation_valid:
  type == "object" and .executionSuccessful == true and
  (if has("toolExecutionNotifications")
   then (.toolExecutionNotifications | notifications_valid) else true end) and
  (if has("toolConfigurationNotifications")
   then (.toolConfigurationNotifications | notifications_valid) else true end);
def below_high:
  if type == "number" then . >= 0 and . < 7.0
  elif type == "string" then
    test("^[0-9]+(\\.[0-9]+)?$") and
    (try (tonumber | . >= 0 and . < 7.0) catch false)
  else false end;
def finding_passes($rules):
  . as $finding |
  if type != "object" or (.ruleId | nonempty_string | not) then false
  else
    [ $rules[] | select(.id == $finding.ruleId) ] as $matches |
    ($matches | length == 1) and
    ($matches[0].properties["security-severity"] | below_high)
  end;
def run_valid:
  type == "object" and
  (.tool | type == "object") and
  (.tool.driver | component_valid) and (.tool.driver.name == "CodeQL") and
  (if .tool | has("extensions") then
     (.tool.extensions | type == "array" and all(.[]; component_valid))
   else true end) and
  (.results | type == "array") and
  (.invocations | type == "array" and length > 0 and all(.[]; invocation_valid)) and
  ([.tool.driver.rules[]?, .tool.extensions[]?.rules[]?] as $rules |
    ($rules | length > 0) and
    (($rules | map(.id) | unique | length) == ($rules | length)) and
    all(.results[]; finding_passes($rules)));
def report_valid:
  type == "object" and .version == "2.1.0" and
  (.runs | type == "array" and length > 0 and all(.[]; run_valid));
(type == "array" and length == 1) and (.[0] | report_valid)
