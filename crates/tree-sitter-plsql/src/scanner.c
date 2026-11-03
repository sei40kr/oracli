// External tokens the regular lexer cannot express:
// - q_string: Oracle's alternative quoting, q'<delim>...<delim>', with paired brackets.
// - slash: a `/` alone on its line, which ends a statement in SQL*Plus and SQLcl.

#include "tree_sitter/parser.h"

enum TokenType { Q_STRING, SLASH };

void *tree_sitter_plsql_external_scanner_create(void) { return NULL; }
void tree_sitter_plsql_external_scanner_destroy(void *payload) {}
unsigned tree_sitter_plsql_external_scanner_serialize(void *payload, char *buffer) { return 0; }
void tree_sitter_plsql_external_scanner_deserialize(void *payload, const char *buffer,
                                                    unsigned length) {}

static bool is_space(int32_t c) { return c == ' ' || c == '\t' || c == '\r'; }

static bool scan_q_string(TSLexer *lexer) {
  if (lexer->lookahead == 'n' || lexer->lookahead == 'N') lexer->advance(lexer, false);
  if (lexer->lookahead != 'q' && lexer->lookahead != 'Q') return false;
  lexer->advance(lexer, false);
  if (lexer->lookahead != '\'') return false;
  lexer->advance(lexer, false);

  int32_t open = lexer->lookahead;
  if (open == 0 || is_space(open) || open == '\n') return false;
  int32_t close = open == '[' ? ']' : open == '{' ? '}' : open == '(' ? ')' : open == '<' ? '>' : open;
  lexer->advance(lexer, false);

  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    lexer->advance(lexer, false);
    if (c == close && lexer->lookahead == '\'') {
      lexer->advance(lexer, false);
      lexer->result_symbol = Q_STRING;
      return true;
    }
  }
  return false;
}

static bool scan_slash(TSLexer *lexer) {
  if (lexer->lookahead != '/') return false;
  lexer->advance(lexer, false);
  lexer->mark_end(lexer);
  while (is_space(lexer->lookahead)) lexer->advance(lexer, false);
  if (lexer->lookahead != '\n' && !lexer->eof(lexer)) return false;
  lexer->result_symbol = SLASH;
  return true;
}

bool tree_sitter_plsql_external_scanner_scan(void *payload, TSLexer *lexer,
                                             const bool *valid_symbols) {
  // Skip leading whitespace ourselves to learn whether the token is the first on its line.
  bool line_start = lexer->get_column(lexer) == 0;
  while (is_space(lexer->lookahead) || lexer->lookahead == '\n') {
    if (lexer->lookahead == '\n') line_start = true;
    lexer->advance(lexer, true);
  }
  if (valid_symbols[SLASH] && line_start && scan_slash(lexer)) return true;
  if (valid_symbols[Q_STRING]) return scan_q_string(lexer);
  return false;
}
