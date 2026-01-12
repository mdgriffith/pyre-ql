/**
 * Filter evaluation for Pyre queries
 */

import type { WhereClause, FilterValue } from './types';

export function evaluateFilter(row: any, where: WhereClause): boolean {
  // Handle $and and $or operators
  if ('$and' in where) {
    const andClauses = where.$and as WhereClause[];
    return andClauses.every(clause => evaluateFilter(row, clause));
  }

  if ('$or' in where) {
    const orClauses = where.$or as WhereClause[];
    return orClauses.some(clause => evaluateFilter(row, clause));
  }

  // Evaluate each field condition
  for (const [field, condition] of Object.entries(where)) {
    if (field === '$and' || field === '$or') {
      continue; // Already handled above
    }

    const fieldValue = row[field];

    if (condition === null || condition === undefined) {
      // Null check
      if (fieldValue !== null && fieldValue !== undefined) {
        return false;
      }
      continue;
    }

    if (typeof condition === 'object' && !Array.isArray(condition)) {
      // Check if it's a nested where clause
      if ('$and' in condition || '$or' in condition || Object.keys(condition).some(k => !k.startsWith('$'))) {
        // Nested where clause
        if (!evaluateFilter(fieldValue || {}, condition as WhereClause)) {
          return false;
        }
        continue;
      }

      // Operator object (e.g., { $eq: value, $gt: value })
      const operators = condition as Record<string, FilterValue>;
      
      for (const [op, opValue] of Object.entries(operators)) {
        if (!evaluateOperator(fieldValue, op, opValue)) {
          return false;
        }
      }
    } else {
      // Simple equality check
      if (!compareValues(fieldValue, condition)) {
        return false;
      }
    }
  }

  return true;
}

function evaluateOperator(lhs: any, operator: string, rhs: FilterValue): boolean {
  switch (operator) {
    case '$eq':
      return compareValues(lhs, rhs);
    case '$ne':
      return !compareValues(lhs, rhs);
    case '$gt':
      return compareNumbers(lhs, rhs) > 0;
    case '$gte':
      return compareNumbers(lhs, rhs) >= 0;
    case '$lt':
      return compareNumbers(lhs, rhs) < 0;
    case '$lte':
      return compareNumbers(lhs, rhs) <= 0;
    case '$in':
      if (!Array.isArray(rhs)) {
        return false;
      }
      return rhs.some(value => compareValues(lhs, value));
    default:
      return false;
  }
}

function compareValues(a: any, b: any): boolean {
  // Handle null/undefined
  if (a === null || a === undefined) {
    return b === null || b === undefined;
  }
  if (b === null || b === undefined) {
    return false;
  }

  // Handle dates
  if (a instanceof Date && b instanceof Date) {
    return a.getTime() === b.getTime();
  }
  if (a instanceof Date) {
    return a.getTime() === new Date(b as string).getTime();
  }
  if (b instanceof Date) {
    return new Date(a as string).getTime() === b.getTime();
  }

  // Handle strings that look like dates
  if (typeof a === 'string' && typeof b === 'string' && /^\d{4}-\d{2}-\d{2}/.test(a) && /^\d{4}-\d{2}-\d{2}/.test(b)) {
    return new Date(a).getTime() === new Date(b).getTime();
  }

  // Standard comparison
  return a === b;
}

function compareNumbers(a: any, b: any): number {
  const numA = typeof a === 'number' ? a : (typeof a === 'string' ? parseFloat(a) : NaN);
  const numB = typeof b === 'number' ? b : (typeof b === 'string' ? parseFloat(b) : NaN);

  if (isNaN(numA) || isNaN(numB)) {
    // Try date comparison
    const dateA = a instanceof Date ? a.getTime() : (typeof a === 'string' ? new Date(a).getTime() : NaN);
    const dateB = b instanceof Date ? b.getTime() : (typeof b === 'string' ? new Date(b).getTime() : NaN);
    
    if (!isNaN(dateA) && !isNaN(dateB)) {
      return dateA - dateB;
    }
    
    return 0;
  }

  return numA - numB;
}

export function sortRows(rows: any[], sorts: Array<{ field: string; direction: 'asc' | 'desc' | 'Asc' | 'Desc' }>): any[] {
  const sorted = [...rows];
  
  sorted.sort((a, b) => {
    for (const sort of sorts) {
      const aVal = a[sort.field];
      const bVal = b[sort.field];
      
      const direction = sort.direction.toLowerCase() === 'desc' ? -1 : 1;
      const comparison = compareValuesForSort(aVal, bVal);
      
      if (comparison !== 0) {
        return comparison * direction;
      }
    }
    
    return 0;
  });
  
  return sorted;
}

function compareValuesForSort(a: any, b: any): number {
  // Handle null/undefined
  if (a === null || a === undefined) {
    return b === null || b === undefined ? 0 : -1;
  }
  if (b === null || b === undefined) {
    return 1;
  }

  // Handle numbers
  if (typeof a === 'number' && typeof b === 'number') {
    return a - b;
  }

  // Handle dates
  const dateA = a instanceof Date ? a.getTime() : (typeof a === 'string' ? new Date(a).getTime() : NaN);
  const dateB = b instanceof Date ? b.getTime() : (typeof b === 'string' ? new Date(b).getTime() : NaN);
  
  if (!isNaN(dateA) && !isNaN(dateB)) {
    return dateA - dateB;
  }

  // Handle strings
  if (typeof a === 'string' && typeof b === 'string') {
    return a.localeCompare(b);
  }

  // Fallback to string comparison
  return String(a).localeCompare(String(b));
}
