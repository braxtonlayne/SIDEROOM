import type { MessageKey } from '../../i18n';
import type { CalcErrorInfo } from './engine';

/** Every code the calculator, units and percent modules can answer with, as a message. */
const KEYS: Record<string, MessageKey> = {
  empty: 'tools.calc.error.empty',
  unexpected_char: 'tools.calc.error.unexpectedChar',
  unexpected_token: 'tools.calc.error.unexpectedToken',
  unexpected_end: 'tools.calc.error.unexpectedEnd',
  unbalanced: 'tools.calc.error.unbalanced',
  divide_by_zero: 'tools.calc.error.divideByZero',
  domain: 'tools.calc.error.domain',
  overflow: 'tools.calc.error.overflow',
  bad_number: 'tools.calc.error.badNumber',
  unknown_name: 'tools.calc.error.unknownName',
  unknown_function: 'tools.calc.error.unknownFunction',
  missing_argument: 'tools.calc.error.missingArgument',
  arity: 'tools.calc.error.arity',
  no_answer: 'tools.calc.error.noAnswer',
  too_deep: 'tools.calc.error.tooDeep',
  too_many_arguments: 'tools.calc.error.tooManyArguments',
  bad_digit: 'tools.calc.error.badDigit',
  literal_too_large: 'tools.calc.error.literalTooLarge',
  shift_range: 'tools.calc.error.shiftRange',
  bad_width: 'tools.calc.error.badWidth',
  bad_base: 'tools.calc.error.badBase',
  category_mismatch: 'tools.calc.error.categoryMismatch',
  unknown_unit: 'tools.calc.error.unknownUnit',
  no_target: 'tools.calc.error.noTarget',
  below_absolute_zero: 'tools.calc.error.belowAbsoluteZero',
  no_phrase: 'tools.calc.error.noPhrase',
};

export const ERROR_CODES = Object.keys(KEYS);

export function errorKey(code: string): MessageKey {
  return KEYS[code] ?? 'tools.calc.error.unknown';
}

/** The variables an error message may use. */
export function errorVars(e: CalcErrorInfo): { detail: string; pos: number } {
  return { detail: e.detail, pos: e.pos + 1 };
}
