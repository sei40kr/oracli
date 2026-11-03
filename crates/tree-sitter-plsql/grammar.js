/**
 * @file Oracle SQL and PL/SQL grammar for tree-sitter
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

// Keywords are case-insensitive and contextual: a keyword is lexed as an identifier wherever
// the grammar does not expect it, so non-reserved words still work as names. They win over
// identifiers by lexical precedence; tree-sitter does not extract case-insensitive regexes as
// keywords of the word token.
const KEYWORDS = [
  'all', 'and', 'any', 'as', 'asc', 'begin', 'between', 'body', 'bulk', 'by', 'cascade', 'case',
  'cast', 'check', 'close', 'collect', 'commit', 'connect', 'constant', 'constraint',
  'constraints', 'create', 'cross', 'cursor', 'date', 'declare', 'default', 'delete', 'desc',
  'distinct', 'drop', 'editionable', 'else', 'elsif', 'end', 'escape', 'except', 'exception',
  'execute', 'exists', 'exit', 'false', 'fetch', 'first', 'for', 'force', 'foreign', 'from',
  'full', 'function', 'global', 'group', 'having', 'if', 'immediate', 'in', 'index', 'inner',
  'insert', 'intersect', 'into', 'is', 'join', 'key', 'last', 'left', 'like', 'locked', 'loop',
  'matched', 'merge', 'minus', 'natural', 'next', 'nocopy', 'nocycle', 'noneditionable',
  'nowait', 'not', 'null', 'nulls', 'of', 'offset', 'on', 'only', 'open', 'or', 'order', 'out',
  'outer', 'over', 'package', 'partition', 'percent', 'primary', 'prior', 'procedure', 'purge',
  'raise', 'read', 'references', 'replace', 'return', 'reverse', 'right', 'rollback', 'row',
  'rows', 'rowtype', 'savepoint', 'select', 'sequence', 'set', 'siblings', 'skip', 'start',
  'synonym', 'table', 'temporary', 'then', 'ties', 'to', 'trigger', 'true', 'truncate', 'type',
  'union', 'unique', 'update', 'user', 'using', 'values', 'view', 'wait', 'when', 'where',
  'while', 'with', 'work',
];

const PREC = {
  or: 1,
  and: 2,
  not: 3,
  compare: 4,
  concat: 5,
  add: 6,
  multiply: 7,
  unary: 8,
  postfix: 9,
};

/** @param {string} word */
function caseInsensitive(word) {
  return new RegExp(
    word
      .split('')
      .map((c) => `[${c.toLowerCase()}${c.toUpperCase()}]`)
      .join(''),
  );
}

/** @param {Rule} rule */
function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

/** @param {Rule} rule */
function parens(rule) {
  return seq('(', rule, ')');
}

module.exports = grammar({
  name: 'plsql',

  word: ($) => $._name,

  extras: ($) => [/\s/, $.comment],

  externals: ($) => [$.q_string, $.slash],

  conflicts: ($) => [
    [$._query_body, $._expression],
  ],

  rules: {
    // The last statement may lack its terminator: the input is often still being typed.
    source_file: ($) =>
      seq(
        repeat(choice(seq($._statement, choice(';', $.slash)), ';', $.slash)),
        optional($._statement),
      ),

    _statement: ($) =>
      choice(
        $.select_statement,
        $.insert_statement,
        $.update_statement,
        $.delete_statement,
        $.merge_statement,
        $.create_table,
        $.create_view,
        $.create_index,
        $.create_subprogram,
        $.drop_statement,
        $.truncate_statement,
        $.commit_statement,
        $.rollback_statement,
        $.savepoint_statement,
        $.block,
      ),

    // Queries

    select_statement: ($) => seq($.query, optional($.for_update_clause)),

    query: ($) =>
      seq(
        optional($.with_clause),
        $._query_body,
        optional($.order_by_clause),
        optional($.row_limiting_clause),
      ),

    _query_body: ($) => choice($.select, $.set_operation, $.subquery),

    set_operation: ($) =>
      prec.left(
        seq(
          $._query_body,
          choice(
            seq($.keyword_union, optional($.keyword_all)),
            $.keyword_intersect,
            $.keyword_minus,
            $.keyword_except,
          ),
          $._query_body,
        ),
      ),

    subquery: ($) => parens($.query),

    with_clause: ($) => seq($.keyword_with, commaSep1($.cte)),

    cte: ($) =>
      seq(
        field('name', $.identifier),
        optional($.column_list),
        $.keyword_as,
        $.subquery,
      ),

    select: ($) =>
      prec.right(
        seq(
          $.keyword_select,
          optional(choice($.keyword_distinct, $.keyword_unique, $.keyword_all)),
          $.select_list,
          optional($.into_clause),
          optional($.from_clause),
          optional($.where_clause),
          optional($.hierarchical_clause),
          optional($.group_by_clause),
          optional($.having_clause),
        ),
      ),

    select_list: ($) => commaSep1(choice($.all_fields, $.select_item)),

    all_fields: ($) => seq(optional(seq($._dotted, '.')), '*'),

    select_item: ($) =>
      seq(
        field('value', $._expression),
        optional(seq(optional($.keyword_as), field('alias', $.identifier))),
      ),

    into_clause: ($) =>
      seq(
        optional(seq($.keyword_bulk, $.keyword_collect)),
        $.keyword_into,
        commaSep1($._target),
      ),

    from_clause: ($) => seq($.keyword_from, commaSep1($._from_item)),

    _from_item: ($) => choice($.table_reference, $.join),

    table_reference: ($) =>
      seq(
        choice($.object_name, $.subquery),
        optional(field('alias', $.identifier)),
      ),

    join: ($) =>
      prec.left(
        seq(
          $._from_item,
          optional(
            choice(
              $.keyword_inner,
              $.keyword_cross,
              $.keyword_natural,
              seq(
                choice($.keyword_left, $.keyword_right, $.keyword_full),
                optional($.keyword_outer),
              ),
            ),
          ),
          $.keyword_join,
          $.table_reference,
          optional(
            choice(
              seq($.keyword_on, $._expression),
              seq($.keyword_using, $.column_list),
            ),
          ),
        ),
      ),

    where_clause: ($) => seq($.keyword_where, $._expression),

    hierarchical_clause: ($) =>
      choice(
        seq($._start_with, optional($._connect_by)),
        seq($._connect_by, optional($._start_with)),
      ),

    _start_with: ($) => seq($.keyword_start, $.keyword_with, $._expression),

    _connect_by: ($) =>
      seq($.keyword_connect, $.keyword_by, optional($.keyword_nocycle), $._expression),

    group_by_clause: ($) =>
      seq($.keyword_group, $.keyword_by, commaSep1($._expression)),

    having_clause: ($) => seq($.keyword_having, $._expression),

    order_by_clause: ($) =>
      seq(
        $.keyword_order,
        optional($.keyword_siblings),
        $.keyword_by,
        commaSep1($.order_item),
      ),

    order_item: ($) =>
      seq(
        $._expression,
        optional(choice($.keyword_asc, $.keyword_desc)),
        optional(seq($.keyword_nulls, choice($.keyword_first, $.keyword_last))),
      ),

    row_limiting_clause: ($) =>
      choice(
        seq($._offset, optional($._fetch)),
        $._fetch,
      ),

    _offset: ($) =>
      seq($.keyword_offset, $._expression, choice($.keyword_row, $.keyword_rows)),

    _fetch: ($) =>
      seq(
        $.keyword_fetch,
        choice($.keyword_first, $.keyword_next),
        optional(seq($._expression, optional($.keyword_percent))),
        choice($.keyword_row, $.keyword_rows),
        choice($.keyword_only, seq($.keyword_with, $.keyword_ties)),
      ),

    for_update_clause: ($) =>
      seq(
        $.keyword_for,
        $.keyword_update,
        optional(seq($.keyword_of, commaSep1($.column_reference))),
        optional(
          choice(
            $.keyword_nowait,
            seq($.keyword_wait, $.number),
            seq($.keyword_skip, $.keyword_locked),
          ),
        ),
      ),

    // DML

    insert_statement: ($) =>
      seq(
        $.keyword_insert,
        $.keyword_into,
        $.table_reference,
        optional($.column_list),
        choice($.values_clause, $.query),
      ),

    values_clause: ($) =>
      seq($.keyword_values, parens(commaSep1($._expression))),

    update_statement: ($) =>
      seq(
        $.keyword_update,
        $.table_reference,
        $.set_clause,
        optional($.where_clause),
      ),

    set_clause: ($) => seq($.keyword_set, commaSep1($.assignment)),

    assignment: ($) =>
      choice(
        seq($.column_reference, '=', $._expression),
        seq($.column_list, '=', $.subquery),
      ),

    delete_statement: ($) =>
      seq(
        $.keyword_delete,
        optional($.keyword_from),
        $.table_reference,
        optional($.where_clause),
      ),

    merge_statement: ($) =>
      seq(
        $.keyword_merge,
        $.keyword_into,
        $.table_reference,
        $.keyword_using,
        $.table_reference,
        $.keyword_on,
        parens($._expression),
        repeat1($.merge_when),
      ),

    merge_when: ($) =>
      seq(
        $.keyword_when,
        optional($.keyword_not),
        $.keyword_matched,
        $.keyword_then,
        choice(
          seq(
            $.keyword_update,
            $.set_clause,
            optional($.where_clause),
            optional(seq($.keyword_delete, $.where_clause)),
          ),
          seq(
            $.keyword_insert,
            optional($.column_list),
            $.values_clause,
            optional($.where_clause),
          ),
        ),
      ),

    // DDL

    create_table: ($) =>
      seq(
        $.keyword_create,
        optional(seq($.keyword_global, $.keyword_temporary)),
        $.keyword_table,
        $.object_name,
        choice(
          parens(commaSep1(choice($.column_definition, $.table_constraint))),
          seq($.keyword_as, $.query),
        ),
      ),

    column_definition: ($) =>
      seq(
        field('name', $.identifier),
        $.data_type,
        repeat($.column_constraint),
      ),

    column_constraint: ($) =>
      choice(
        seq($.keyword_default, $._expression),
        seq($.keyword_constraint, $.identifier),
        seq(optional($.keyword_not), $.keyword_null),
        seq($.keyword_primary, $.keyword_key),
        $.keyword_unique,
        seq($.keyword_references, $.object_name, optional($.column_list)),
        seq($.keyword_check, parens($._expression)),
      ),

    table_constraint: ($) =>
      seq(
        optional(seq($.keyword_constraint, $.identifier)),
        choice(
          seq($.keyword_primary, $.keyword_key, $.column_list),
          seq($.keyword_unique, $.column_list),
          seq(
            $.keyword_foreign,
            $.keyword_key,
            $.column_list,
            $.keyword_references,
            $.object_name,
            optional($.column_list),
          ),
          seq($.keyword_check, parens($._expression)),
        ),
      ),

    create_view: ($) =>
      prec.right(seq(
        $.keyword_create,
        optional($._or_replace),
        optional(seq(optional($.keyword_not), $.keyword_force)),
        $.keyword_view,
        $.object_name,
        optional($.column_list),
        $.keyword_as,
        $.query,
        optional(seq($.keyword_with, $.keyword_read, $.keyword_only)),
      )),

    create_index: ($) =>
      seq(
        $.keyword_create,
        optional($.keyword_unique),
        $.keyword_index,
        $.object_name,
        $.keyword_on,
        $.object_name,
        parens(commaSep1($.order_item)),
      ),

    drop_statement: ($) =>
      seq(
        $.keyword_drop,
        choice(
          $.keyword_table,
          $.keyword_view,
          $.keyword_index,
          $.keyword_sequence,
          $.keyword_synonym,
          $.keyword_procedure,
          $.keyword_function,
          seq($.keyword_package, optional($.keyword_body)),
          $.keyword_trigger,
          $.keyword_type,
          $.keyword_user,
        ),
        $.object_name,
        optional(seq($.keyword_cascade, $.keyword_constraints)),
        optional($.keyword_purge),
      ),

    truncate_statement: ($) =>
      seq($.keyword_truncate, $.keyword_table, $.object_name),

    commit_statement: ($) => seq($.keyword_commit, optional($.keyword_work)),

    rollback_statement: ($) =>
      seq(
        $.keyword_rollback,
        optional($.keyword_work),
        optional(seq($.keyword_to, optional($.keyword_savepoint), $.identifier)),
      ),

    savepoint_statement: ($) => seq($.keyword_savepoint, $.identifier),

    _or_replace: ($) => seq($.keyword_or, $.keyword_replace),

    // PL/SQL

    create_subprogram: ($) =>
      seq(
        $.keyword_create,
        optional($._or_replace),
        optional(choice($.keyword_editionable, $.keyword_noneditionable)),
        choice(
          seq($.keyword_procedure, $.object_name, optional($.parameter_list)),
          seq(
            $.keyword_function,
            $.object_name,
            optional($.parameter_list),
            $.keyword_return,
            $.data_type,
          ),
        ),
        choice($.keyword_is, $.keyword_as),
        repeat($._declaration),
        $._body,
      ),

    parameter_list: ($) => parens(commaSep1($.parameter)),

    parameter: ($) =>
      seq(
        field('name', $.identifier),
        optional(choice(seq($.keyword_in, optional($.keyword_out)), $.keyword_out)),
        optional($.keyword_nocopy),
        $.data_type,
        optional(seq(choice(':=', $.keyword_default), $._expression)),
      ),

    block: ($) =>
      seq(optional(seq($.keyword_declare, repeat($._declaration))), $._body),

    _body: ($) =>
      seq(
        $.keyword_begin,
        repeat($._plsql_statement),
        optional($.exception_section),
        $.keyword_end,
        optional($.identifier),
        ';',
      ),

    exception_section: ($) => seq($.keyword_exception, repeat1($.exception_handler)),

    exception_handler: ($) =>
      seq(
        $.keyword_when,
        $._dotted,
        repeat(seq($.keyword_or, $._dotted)),
        $.keyword_then,
        repeat1($._plsql_statement),
      ),

    _declaration: ($) =>
      choice(
        $.variable_declaration,
        $.cursor_declaration,
        $.exception_declaration,
      ),

    variable_declaration: ($) =>
      seq(
        field('name', $.identifier),
        optional($.keyword_constant),
        $.data_type,
        optional(seq($.keyword_not, $.keyword_null)),
        optional(seq(choice(':=', $.keyword_default), $._expression)),
        ';',
      ),

    cursor_declaration: ($) =>
      seq(
        $.keyword_cursor,
        field('name', $.identifier),
        optional($.parameter_list),
        $.keyword_is,
        $.query,
        ';',
      ),

    exception_declaration: ($) =>
      seq(field('name', $.identifier), $.keyword_exception, ';'),

    _plsql_statement: ($) =>
      choice(
        seq(
          choice(
            $.select_statement,
            $.insert_statement,
            $.update_statement,
            $.delete_statement,
            $.merge_statement,
            $.commit_statement,
            $.rollback_statement,
            $.savepoint_statement,
            $.assignment_statement,
            $.call_statement,
            $.null_statement,
            $.if_statement,
            $.loop_statement,
            $.exit_statement,
            $.return_statement,
            $.raise_statement,
            $.execute_immediate,
            $.open_statement,
            $.fetch_statement,
            $.close_statement,
          ),
          ';',
        ),
        $.block,
      ),

    assignment_statement: ($) => seq($._target, ':=', $._expression),

    call_statement: ($) => choice($.function_call, $._dotted),

    null_statement: ($) => $.keyword_null,

    if_statement: ($) =>
      seq(
        $.keyword_if,
        $._expression,
        $.keyword_then,
        repeat1($._plsql_statement),
        repeat(
          seq(
            $.keyword_elsif,
            $._expression,
            $.keyword_then,
            repeat1($._plsql_statement),
          ),
        ),
        optional(seq($.keyword_else, repeat1($._plsql_statement))),
        $.keyword_end,
        $.keyword_if,
      ),

    loop_statement: ($) =>
      seq(
        optional(
          choice(
            seq($.keyword_while, $._expression),
            seq(
              $.keyword_for,
              field('variable', $.identifier),
              $.keyword_in,
              optional($.keyword_reverse),
              choice(
                seq($._expression, '..', $._expression),
                $.subquery,
              ),
            ),
          ),
        ),
        $.keyword_loop,
        repeat1($._plsql_statement),
        $.keyword_end,
        $.keyword_loop,
        optional($.identifier),
      ),

    exit_statement: ($) =>
      seq(
        $.keyword_exit,
        optional($.identifier),
        optional(seq($.keyword_when, $._expression)),
      ),

    return_statement: ($) => seq($.keyword_return, optional($._expression)),

    raise_statement: ($) => seq($.keyword_raise, optional($._dotted)),

    execute_immediate: ($) =>
      seq(
        $.keyword_execute,
        $.keyword_immediate,
        $._expression,
        optional($.into_clause),
        optional(seq($.keyword_using, commaSep1($._expression))),
      ),

    open_statement: ($) =>
      seq(
        $.keyword_open,
        $._dotted,
        optional(seq($.keyword_for, choice($.query, $._expression))),
      ),

    fetch_statement: ($) => seq($.keyword_fetch, $._dotted, $.into_clause),

    close_statement: ($) => seq($.keyword_close, $._dotted),

    _target: ($) => choice($.column_reference, $.bind_variable),

    data_type: ($) =>
      prec.right(
        choice(
          seq(
            $._dotted,
            optional(
              parens(
                seq(
                  commaSep1(choice($.number, '*')),
                  optional(alias(choice(/[bB][yY][tT][eE]/, /[cC][hH][aA][rR]/), $.unit)),
                ),
              ),
            ),
          ),
          seq($._dotted, '%', choice($.keyword_type, $.keyword_rowtype)),
        ),
      ),

    // Expressions

    _expression: ($) =>
      choice(
        $.literal,
        $.column_reference,
        $.bind_variable,
        $.function_call,
        $.case_expression,
        $.cast_expression,
        $.unary_expression,
        $.binary_expression,
        $.comparison,
        $.parenthesized_expression,
        $.subquery,
        $.exists,
      ),

    literal: ($) =>
      choice(
        $.number,
        $.string,
        $.q_string,
        $.keyword_null,
        $.keyword_true,
        $.keyword_false,
        seq($.keyword_date, $.string),
      ),

    number: (_) =>
      token(
        seq(
          choice(/\d+(\.\d*)?/, /\.\d+/),
          optional(/[eE][+-]?\d+/),
          optional(/[fFdD]/),
        ),
      ),

    // An unterminated string runs to the end of the input, where the user is still typing it.
    string: (_) => /[nN]?'([^']|'')*'?/,

    bind_variable: (_) => /:[A-Za-z0-9_$#]+/,

    column_reference: ($) => seq($._dotted, optional('(+)')),

    function_call: ($) =>
      prec(
        PREC.postfix,
        seq(
          field('name', $._dotted),
          parens(
            optional(
              choice(
                '*',
                seq(
                  optional(choice($.keyword_distinct, $.keyword_all)),
                  commaSep1($.argument),
                ),
              ),
            ),
          ),
          optional($.over_clause),
        ),
      ),

    argument: ($) => seq(optional(seq($.identifier, '=>')), $._expression),

    over_clause: ($) =>
      seq(
        $.keyword_over,
        parens(
          seq(
            optional(
              seq($.keyword_partition, $.keyword_by, commaSep1($._expression)),
            ),
            optional($.order_by_clause),
          ),
        ),
      ),

    case_expression: ($) =>
      seq(
        $.keyword_case,
        optional($._expression),
        repeat1(seq($.keyword_when, $._expression, $.keyword_then, $._expression)),
        optional(seq($.keyword_else, $._expression)),
        $.keyword_end,
      ),

    cast_expression: ($) =>
      seq($.keyword_cast, parens(seq($._expression, $.keyword_as, $.data_type))),

    unary_expression: ($) =>
      choice(
        prec(PREC.not, seq($.keyword_not, $._expression)),
        prec(PREC.unary, seq(choice('-', '+', $.keyword_prior), $._expression)),
      ),

    binary_expression: ($) =>
      choice(
        prec.left(PREC.or, seq($._expression, $.keyword_or, $._expression)),
        prec.left(PREC.and, seq($._expression, $.keyword_and, $._expression)),
        prec.left(PREC.concat, seq($._expression, '||', $._expression)),
        prec.left(PREC.add, seq($._expression, choice('+', '-'), $._expression)),
        prec.left(PREC.multiply, seq($._expression, choice('*', '/'), $._expression)),
      ),

    comparison: ($) =>
      choice(
        prec.left(
          PREC.compare,
          seq(
            $._expression,
            choice('=', '<>', '!=', '^=', '<', '>', '<=', '>='),
            optional(choice($.keyword_any, $.keyword_all)),
            $._expression,
          ),
        ),
        prec.left(
          PREC.compare,
          seq(
            $._expression,
            $.keyword_is,
            optional($.keyword_not),
            $.keyword_null,
          ),
        ),
        prec.left(
          PREC.compare,
          seq(
            $._expression,
            optional($.keyword_not),
            $.keyword_like,
            $._expression,
            optional(seq($.keyword_escape, $._expression)),
          ),
        ),
        prec.left(
          PREC.compare,
          seq(
            $._expression,
            optional($.keyword_not),
            $.keyword_in,
            choice($.subquery, parens(commaSep1($._expression))),
          ),
        ),
        prec.left(
          PREC.compare,
          seq(
            $._expression,
            optional($.keyword_not),
            $.keyword_between,
            $._expression,
            $.keyword_and,
            $._expression,
          ),
        ),
      ),

    parenthesized_expression: ($) => parens(commaSep1($._expression)),

    exists: ($) => seq($.keyword_exists, $.subquery),

    // Names

    object_name: ($) =>
      seq(
        optional(seq(field('schema', $.identifier), '.')),
        field('name', $.identifier),
        optional(seq('@', field('dblink', $.identifier))),
      ),

    column_list: ($) => parens(commaSep1($.identifier)),

    _dotted: ($) => prec.right(seq($.identifier, repeat(seq('.', $.identifier)))),

    identifier: ($) => choice($._name, /"[^"]*"/),

    _name: (_) => /[A-Za-z][A-Za-z0-9_$#]*/,

    comment: (_) =>
      token(choice(seq('--', /.*/), seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'))),

    ...Object.fromEntries(
      KEYWORDS.map((k) => [`keyword_${k}`, (_) => token(prec(1, caseInsensitive(k)))]),
    ),
  },
});
