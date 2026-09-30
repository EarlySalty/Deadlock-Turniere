# Fail closed for malformed/incomplete reports and unresolved or unscored findings.
# Reporting uploads are deliberately unrelated to this mandatory decision.
def rules_valid:
  (type == "array") and (length > 0) and
  all(.[]; (type == "object") and (.id | type == "string" and length > 0));
def finding_passes($rules):
  . as $finding |
  if ($finding | type) != "object" then false
  else
    [ $rules[] | select(.id == $finding.ruleId) ] as $matches |
    if ($matches | length) != 1 then false
    else
      $matches[0].properties["security-severity"] as $score |
      (($score | type) == "string" or ($score | type) == "number") and
      (try ($score | tonumber | . >= 0 and . < 7.0) catch false)
    end
  end;
# CodeQL query-pack rules live in tool.extensions; driver.rules can legitimately
# be empty. Libraries without their own rules are normal, absent catalogs are not.
def catalog:
  .tool.driver.rules + ([.tool.extensions[]? | (.rules // [])[]] // []);
(type == "object") and (.version == "2.1.0") and
(.runs | type == "array" and length > 0) and
all(.runs[];
  (type == "object") and
  (.tool.driver.rules | type == "array") and
  (catalog | rules_valid) and
  (.results | type == "array") and
  all(.invocations[]?; .executionSuccessful == true) and
  (catalog as $rules | all(.results[]; finding_passes($rules))))
