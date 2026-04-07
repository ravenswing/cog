from pathlib import Path

import polars as pl


def main() -> None:
    data = Path.cwd().parent / "data"

    # games = pl.scan_csv(data / "games.csv", escape="\\", quote_char='"')
    # print(games.head().collect())
    # Need to be able to deal with embedded dictionaries.

    # genres = pl.scan_csv(data / "genres.csv").group_by("app_id")
    # print(genres.head().collect())
    # Need to collapse by ID into one column
    # Need to replace all low-populated ones with Other

    # tags = pl.scan_csv(data / "tags.csv").group_by("app_id")
    # print(tags.head().collect())
    # Need to collapse by ID into one column

    # categories = pl.scan_csv(data / "categories.csv")
    # print(categories.head().collect())
    # Need to collapse by ID into one column

    # reviews = pl.scan_csv(data / "reviews.csv")
    # print(reviews.head().collect())
    # Need to be able to not read the last entry in each line of the csv.

    steamspy = pl.scan_csv(data / "steamspy_insights.csv", null_values=["\\N"])
    print(steamspy.head().collect())
    # Note duplicate "genres" column


if __name__ == "__main__":
    main()
