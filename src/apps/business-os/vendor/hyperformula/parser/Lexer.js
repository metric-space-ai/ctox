/**
 * CTOX Business OS - HyperFormula ESM Port
 * parser/Lexer.js — Tokenizer for formula strings.
 *
 * ref: hyperformula/src/parser/LexerConfig.ts:1-200
 */

export function tokenize(str) {
  const tokens = [];
  let i = 0;

  while (i < str.length) {
    const char = str[i];

    // Skip whitespaces
    if (/\s/.test(char)) {
      i++;
      continue;
    }

    // String literal parsing
    if (char === '"') {
      let val = "";
      i++;
      while (i < str.length && str[i] !== '"') {
        if (str[i] === '\\' && str[i + 1] === '"') {
          val += '"';
          i += 2;
        } else {
          val += str[i];
          i++;
        }
      }
      i++; // skip closing quote
      tokens.push({ type: 'STRING', value: val });
      continue;
    }

    // Number literal parsing
    if (/[0-9.]/.test(char)) {
      let val = "";
      while (i < str.length && /[0-9.]/.test(str[i])) {
        val += str[i];
        i++;
      }
      tokens.push({ type: 'NUMBER', value: parseFloat(val) });
      continue;
    }

    // Math Operators and Structural delimiters
    if (char === ',' || char === '(' || char === ')' || char === '+' || char === '-' || char === '*' || char === '/' || char === '^' || char === '&') {
      tokens.push({ type: 'OPERATOR', value: char });
      i++;
      continue;
    }

    // Logical Comparators (<=, >=, <>, <, >, =)
    if (char === '<' || char === '>' || char === '=') {
      let val = char;
      if (char === '<' && (str[i + 1] === '=' || str[i + 1] === '>')) {
        val += str[i + 1];
        i += 2;
      } else if (char === '>' && str[i + 1] === '=') {
        val += str[i + 1];
        i += 2;
      } else {
        i++;
      }
      tokens.push({ type: 'OPERATOR', value: val });
      continue;
    }

    // Identifiers (Cells, Ranges, Functions)
    if (/[A-Za-z_0-9!]/.test(char) || char === '\'') {
      let val = "";

      // Quoted sheet name handling ('Sheet One'!A1)
      if (char === '\'') {
        val += char;
        i++;
        while (i < str.length && str[i] !== '\'') {
          val += str[i];
          i++;
        }
        val += '\'';
        i++;
      }

      while (i < str.length && /[A-Za-z_0-9!:$]/.test(str[i])) {
        val += str[i];
        i++;
      }

      if (i < str.length && str[i] === '(') {
        tokens.push({ type: 'FUNCTION', value: val.toUpperCase() });
      } else if (val.includes(':')) {
        tokens.push({ type: 'RANGE', value: val });
      } else {
        const upperVal = val.toUpperCase();
        if (upperVal === 'TRUE') {
          tokens.push({ type: 'BOOLEAN', value: true });
        } else if (upperVal === 'FALSE') {
          tokens.push({ type: 'BOOLEAN', value: false });
        } else {
          tokens.push({ type: 'CELL', value: val });
        }
      }
      continue;
    }

    i++;
  }
  return tokens;
}
