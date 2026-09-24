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
(type == "object") and (.version == "2.1.0") and
(.runs | type == "array" and length > 0) and
all(.runs[];
  (type == "object") and
  (.tool.driver.rules | rules_valid) and
  (.results | type == "array") and
  all(.invocations[]?; .executionSuccessful == true) and
  (.tool.driver.rules as $rules | all(.results[]; finding_passes($rules))))
