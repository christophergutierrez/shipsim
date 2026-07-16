#!/usr/bin/env python3
"""
Parse simulation report JSON and print per-engagement win rate table.

Usage:
    python3 tools/claims_report.py <report.json>

Output:
    - Per-engagement table with n, Won%, Lost%, InProgress%, and 95% binomial CI half-width
    - Aggregate stats (matches, termination_rate, win_rate)
    - Rubric statuses
"""

import json
import sys
import math
from collections import defaultdict


def binomial_half_width(p, n):
    """
    Compute 95% binomial confidence interval half-width.

    half_width = 1.96 * sqrt(p * (1-p) / n) * 100
    """
    if n == 0:
        return 0.0
    return 1.96 * math.sqrt(p * (1 - p) / n) * 100


def parse_report(filepath):
    """Load and validate the report JSON structure."""
    try:
        with open(filepath, 'r') as f:
            report = json.load(f)
    except FileNotFoundError:
        print(f"Error: file not found: {filepath}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error: invalid JSON in {filepath}: {e}", file=sys.stderr)
        sys.exit(1)

    # Validate required top-level keys
    required_keys = ['name', 'aggregate', 'matches']
    for key in required_keys:
        if key not in report:
            if key == 'matches':
                # 'matches' might be empty or missing; handle gracefully
                if key not in report:
                    print(f"Error: report missing required key '{key}'", file=sys.stderr)
                    sys.exit(1)
            else:
                print(f"Error: report missing required key '{key}'", file=sys.stderr)
                sys.exit(1)

    # Validate matches structure
    if not isinstance(report.get('matches'), list):
        print("Error: 'matches' key is not a list or is missing", file=sys.stderr)
        sys.exit(1)

    if len(report['matches']) == 0:
        print("Error: 'matches' list is empty", file=sys.stderr)
        sys.exit(1)

    return report


def categorize_matches(matches):
    """
    Categorize matches by engagement and outcome.

    Returns a dict: {engagement_name -> {'won': count, 'lost': count, 'in_progress': count}}
    """
    engagement_stats = defaultdict(lambda: {'won': 0, 'lost': 0, 'in_progress': 0})

    for match in matches:
        engagement = match.get('engagement', 'unknown')
        status = match.get('status', 'unknown').lower()

        if status == 'won':
            engagement_stats[engagement]['won'] += 1
        elif status == 'lost':
            engagement_stats[engagement]['lost'] += 1
        elif status == 'inprogress':
            engagement_stats[engagement]['in_progress'] += 1

    return engagement_stats


def format_table(engagement_stats):
    """Format engagement statistics as an aligned text table."""
    if not engagement_stats:
        return ""

    # Prepare rows
    rows = []
    for engagement in sorted(engagement_stats.keys()):
        stats = engagement_stats[engagement]
        n = stats['won'] + stats['lost'] + stats['in_progress']

        won_pct = (stats['won'] / n * 100) if n > 0 else 0.0
        lost_pct = (stats['lost'] / n * 100) if n > 0 else 0.0
        ip_pct = (stats['in_progress'] / n * 100) if n > 0 else 0.0

        # Half-width for Won%
        p_won = stats['won'] / n if n > 0 else 0
        half_width = binomial_half_width(p_won, n)

        rows.append({
            'engagement': engagement,
            'n': n,
            'won_pct': won_pct,
            'lost_pct': lost_pct,
            'ip_pct': ip_pct,
            'half_width': half_width
        })

    # Compute column widths
    col_widths = {
        'engagement': max(len('Engagement'), max(len(r['engagement']) for r in rows)),
        'n': max(len('n'), len(str(max(r['n'] for r in rows)))),
        'won_pct': len('Won%'),
        'lost_pct': len('Lost%'),
        'ip_pct': len('InProgress%'),
        'half_width': len('±95% CI half-width on Won%')
    }

    # Build header
    header = (
        f"{'Engagement':<{col_widths['engagement']}}  "
        f"{'n':>{col_widths['n']}}  "
        f"{'Won%':>8}  "
        f"{'Lost%':>8}  "
        f"{'InProgress%':>12}  "
        f"±95% half-width"
    )

    # Build separator
    sep = '-' * len(header)

    # Build rows
    lines = [header, sep]
    for row in rows:
        line = (
            f"{row['engagement']:<{col_widths['engagement']}}  "
            f"{row['n']:>{col_widths['n']}}  "
            f"{row['won_pct']:>8.1f}  "
            f"{row['lost_pct']:>8.1f}  "
            f"{row['ip_pct']:>12.1f}  "
            f"±{row['half_width']:.1f}pp"
        )
        lines.append(line)

    return '\n'.join(lines)


def main():
    if len(sys.argv) != 2:
        print("Usage: python3 tools/claims_report.py <report.json>", file=sys.stderr)
        sys.exit(1)

    filepath = sys.argv[1]
    report = parse_report(filepath)

    # Print report name
    print(f"Report: {report.get('name', 'unnamed')}")
    print()

    # Parse and display per-engagement stats
    engagement_stats = categorize_matches(report['matches'])
    print("Per-engagement results:")
    print(format_table(engagement_stats))
    print()

    # Aggregate stats
    aggregate = report.get('aggregate', {})
    print("Aggregate stats:")
    print(f"  Matches:          {aggregate.get('matches', 'N/A')}")
    print(f"  Termination rate: {aggregate.get('termination_rate', 'N/A'):.4f}")
    print(f"  Win rate (overall): {aggregate.get('win_rate', 'N/A'):.4f}")
    print()

    # Rubrics
    rubrics = report.get('rubrics', [])
    if rubrics:
        print("Rubric results:")
        for rubric in rubrics:
            rubric_id = rubric.get('id', 'unknown')
            passed = rubric.get('passed', None)
            status = 'PASS' if passed is True else ('FAIL' if passed is False else 'unknown')
            print(f"  {rubric_id}: {status}")
    else:
        print("No rubrics in report.")


if __name__ == '__main__':
    main()
