/**
 * The native core function set registered by the engine itself (WP-09 adds the rest).
 */
import { aggregateFunctions } from './builtins/aggregate.ts';
import { dateFunctions } from './builtins/datetime.ts';
import { dynamicFunctions } from './builtins/dynamic.ts';
import { lambdaFunctions } from './builtins/lambda.ts';
import { logicalFunctions } from './builtins/logical.ts';
import { lookupFunctions } from './builtins/lookup.ts';
import { mathFunctions } from './builtins/math.ts';
import { textFunctions } from './builtins/text.ts';
import type { FunctionDescriptor } from './registry.ts';

export const CORE_FUNCTIONS: readonly FunctionDescriptor[] = [
  ...aggregateFunctions,
  ...logicalFunctions,
  ...mathFunctions,
  ...textFunctions,
  ...lookupFunctions,
  ...dateFunctions,
  ...dynamicFunctions,
  ...lambdaFunctions,
];
