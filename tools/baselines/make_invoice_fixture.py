#!/usr/bin/env python3
"""Build the filled-voucher invoice fixture from the public-domain GSA SF-1035.

The v0 corpus needs an invoice-class ruled-table fixture. Blank invoice forms
carry no line items, and real-world invoices are rarely redistributable, so
this script fills the public-domain GSA SF-1035 continuation sheet (a
column-ruled line-item voucher) with deterministic line items and flattens
the form fields into page content. The output is a real PDF document whose
ruled line-item table has known, hand-checkable contents.

Usage:
  curl -L -o /tmp/sf1035.pdf https://www.gsa.gov/system/files/SF1035-73.pdf
  .glyphrush-baselines/venv/bin/python tools/baselines/make_invoice_fixture.py \
      /tmp/sf1035.pdf test/v0/forms/gsa-sf1035-filled-voucher.pdf
"""

import sys

import fitz

LINE_ITEMS = [
    ("INV-2026-0117", "2026-01-05", "Network switch, 24-port managed", "4", "385.00", "EA", "1,540.00"),
    ("INV-2026-0117", "2026-01-05", "Cat6 patch cable, 3 meter", "60", "8.25", "EA", "495.00"),
    ("INV-2026-0118", "2026-01-12", "Rack installation labor", "16", "95.00", "HR", "1,520.00"),
    ("INV-2026-0118", "2026-01-12", "Cable management tray", "8", "27.50", "EA", "220.00"),
    ("INV-2026-0121", "2026-01-19", "Network configuration services", "12", "110.00", "HR", "1,320.00"),
    ("INV-2026-0121", "2026-01-26", "On-site acceptance testing", "6", "120.00", "HR", "720.00"),
]

HEADER_FIELDS = {
    "VoucherNo": "V-100482",
    "ScheduleNo": "SCH-77",
    "SheetNo": "2",
    "Establishment": "GENERAL SERVICES ADMINISTRATION, REGION 5, CHICAGO, IL",
}


def field(page, suffix):
    for widget in page.widgets():
        if widget.field_name.endswith(suffix):
            return widget
    raise KeyError(suffix)


def main():
    source, target = sys.argv[1], sys.argv[2]
    doc = fitz.open(source)
    page = doc[0]

    for suffix, value in HEADER_FIELDS.items():
        widget = field(page, f"{suffix}[0]")
        widget.field_value = value
        widget.update()

    for row, item in enumerate(LINE_ITEMS, start=1):
        number, date, description, quantity, cost, per, amount = item
        for suffix, value in [
            (f"NumberofOrder{row}[0]", number),
            (f"DateofDelivery{row}[0]", date),
            (
                f"ArticlesorServices{row}[0]"
                if row != 1
                else "ArticlesorService1[0]",
                description,
            ),
            (f"Quantity{row}[0]", quantity),
            (f"Cost{row}[0]", cost),
            (f"Per{row}[0]", per),
            (f"Amount{row}[0]", amount),
        ]:
            try:
                widget = field(page, suffix)
            except KeyError:
                # The 1973 form names the first description field
                # ArticlesorService1; later rows use ArticlesorServices<N>.
                widget = field(page, f"ArticlesorServices{row}[0]")
            widget.field_value = value
            widget.update()

    total_row = len(LINE_ITEMS) + 1
    field(page, f"ArticlesorServices{total_row}[0]").field_value = "TOTAL"
    field(page, f"ArticlesorServices{total_row}[0]").update()
    field(page, f"Amount{total_row}[0]").field_value = "5,815.00"
    field(page, f"Amount{total_row}[0]").update()

    doc.bake()
    doc.save(target, deflate=True)
    print(f"wrote {target}")


if __name__ == "__main__":
    main()
