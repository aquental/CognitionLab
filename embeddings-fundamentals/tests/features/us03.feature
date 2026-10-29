@us03 @evaluation
Feature: Compare text, image, and combined retrieval fairly
  As an evaluator
  I want to reuse fixed queries and relevance labels
  So that mode comparisons measure the configured retrieval behavior

  Scenario Outline: Evaluate a retrieval mode against the frozen dataset
    Given ten frozen Portuguese queries and frozen relevance labels
    And a compatible fixture index with at least three candidates for each query in "<mode>" mode
    When I evaluate "<mode>" mode
    Then the report contains exactly ten query entries with three ranked results each
    And each result includes its source, page or asset, score, relevance label, and relevance note
    And the report records the same query-set and relevance-label checksums as the inputs
    And Precision@3, Hit@3, and nDCG@3 match the independently precomputed fixture values within 0.000001

    @m2
    Examples: Text
      | mode       |
      | text-only  |

    @m3
    Examples: Image
      | mode       |
      | image-only |

  @m2
  Scenario: Report low relevance without hiding a completed experiment
    Given a completed fixture evaluation whose retrieved items all have relevance label 0
    When I generate the comparison report
    Then Precision@3, Hit@3, and nDCG@3 are all 0
    And the evaluation status is "completed"
    And the report preserves the irrelevant results and their failure notes
