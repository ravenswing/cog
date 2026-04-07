from ast import literal_eval

import numpy as np
import pandas as pd


def str_to_price(input):
    # NEED TO CHECK CURRENCY!!!
    if "{" in input:
        data = literal_eval(input)
        return np.float64(data["final"]) / 100
    else:
        return np.nan


def str_to_currency(input):
    if "{" in input:
        data = literal_eval(input)
        return data["currency"]
    else:
        return np.nan


def str_to_users(input):
    if input == np.nan:
        return input
    else:
        vals = [s.replace(",", "") for s in input.split()]
        lower = float(vals[0])
        upper = float(vals[-1])
        return np.mean([lower, upper])


def main() -> None:
    n_entries = 10000

    # INITIAL IMPORT -> GAMES =================================================
    df = pd.read_csv(
        "./data/games.csv",
        low_memory=False,
        escapechar="\\",
        quoting=1,
        quotechar='"',
        on_bad_lines="warn",
    )
    # print(df.info())
    df.drop("is_free", axis=1, inplace=True)

    df["n_languages"] = df.languages.apply(lambda x: len(x.split(",")))
    df.drop("languages", axis=1, inplace=True)

    df["price"] = df.price_overview.apply(str_to_price)
    df["currency"] = df.price_overview.apply(str_to_currency)
    df.drop("price_overview", axis=1, inplace=True)

    # TODO! -> convert currencies if needed
    # print(df.currency.dropna().value_counts())

    # ! Convert to datetime
    # ! Extract year from datetime
    df["release_year"] = pd.to_datetime(df.release_date.replace("N", np.nan)).dt.year
    df.drop("release_date", axis=1, inplace=True)

    # GENRES ==================================================================
    df2 = pd.read_csv(
        "./data/genres.csv",
        low_memory=False,
        escapechar="\\",
        quoting=1,
        quotechar='"',
        on_bad_lines="warn",
    ).sort_values(by="app_id")
    # TODO! -> this is a poor solution
    df2.drop_duplicates(subset="app_id", inplace=True)
    to_other = df2.genre.value_counts().loc[lambda x: x <= 50].index
    df2["genre"] = df2.genre.replace(to_other, "Other")
    # print(sum(df2.duplicated(subset="app_id")) == 0)
    # print(df2[df2.duplicated(subset="app_id")])

    df = df.merge(df2, how="left", on="app_id")
    del df2

    # REVIEWS =================================================================
    df3 = pd.read_csv(
        "./data/reviews.csv",
        low_memory=False,
        escapechar="\\",
        quoting=1,
        quotechar='"',
        on_bad_lines="warn",
    ).sort_values(by="app_id")

    df3.replace("N", np.nan, inplace=True)
    df3 = df3.astype({"positive": float, "total": float, "recommendations": float})
    df3["review_perc"] = df3.positive / df3.total * 100

    df3 = df3[["app_id", "review_perc", "recommendations"]]

    assert sum(df3.duplicated(subset="app_id")) == 0, (
        "Duplicates found in reviews.csv App_ID"
    )
    df = df.merge(df3, how="left", on="app_id")
    del df3

    # STEAMSPY INSIGHTS =======================================================
    df4 = pd.read_csv(
        "./data/steamspy_insights.csv",
        low_memory=False,
        escapechar="\\",
        quoting=1,
        quotechar='"',
        on_bad_lines="warn",
    ).sort_values(by="app_id")

    df4.replace("N", np.nan, inplace=True)
    # ! Filter columns based on regex
    # print(df4.filter(regex="playtime_*").value_counts())
    # ALSO -> df[df.columns[df.columns.str.startswith('d')]]

    # ! Rename a column
    # df4.rename(columns={"playtime_average_forever": "average_playtime"}, inplace=True)
    df4["est_users"] = df4.owners_range.apply(str_to_users)
    # print(df4.owners_range.loc[0], df4.est_users.loc[0])
    df4 = df4[["app_id", "est_users"]]

    assert sum(df4.duplicated(subset="app_id")) == 0, (
        "Duplicates found in steamspy_insights.csv App_ID"
    )
    df = df.merge(df4, how="left", on="app_id")
    del df4

    # CATEGORIES ==============================================================
    df5 = pd.read_csv(
        "./data/categories.csv",
        low_memory=False,
        escapechar="\\",
        quoting=1,
        quotechar='"',
        on_bad_lines="warn",
    ).sort_values(by="app_id")

    df5.replace("N", np.nan, inplace=True)
    # print(df5.category.value_counts().head(20))

    # ! collapse dataframe with multiple lines per entry
    # ! aggregates all values into 1 column in a comma sep. list
    df5 = df5.groupby("app_id").agg(", ".join).reset_index()
    for s in [
        "Multi-player",
        "PvP",
        "Co-op",
        "Full controller support",
        "Steam Achievements",
        "In-App Purchases",
    ]:
        df5[f"has_{s.lower().replace(' ', '_')}"] = df5.category.str.contains(s).astype(
            int
        )

    df5.drop("category", axis=1, inplace=True)

    assert sum(df5.duplicated(subset="app_id")) == 0, (
        "Duplicates found in steamspy_insights.csv App_ID"
    )
    df = df.merge(df5, how="left", on="app_id")
    del df5

    # FINAL CLEANING ==========================================================
    # print(df.type.unique())
    df.drop(df[df.type == "demo"].index, inplace=True)
    # print(df.type.unique())
    df.drop("type", axis=1, inplace=True)

    # TODO! could do this earlier with comparison to is_free column
    df.price = df.price.fillna(0)
    df.drop("currency", axis=1, inplace=True)
    # TODO! This could be looked at emore in the future:
    # default axis=0 i.e. rows
    df.dropna(inplace=True)

    df = df.sort_values(by="est_users", ascending=False).iloc[:n_entries]
    assert all([n == n_entries for n in df.notna().sum().to_list()]), (
        "NaNs found in final DataFrame"
    )

    output = "classification"

    if output == "regression":
        df.to_csv("./data/steam_data_clean_reg.csv", index=False)
    elif output == "classification":
        df[f"review_class"] = pd.cut(
            df["review_perc"],
            [0, 70, 100],
            labels=["Mixed", "Positive"],
        )
        df.drop("review_perc", axis=1, inplace=True)
        df.to_csv("./data/steam_data_clean_cls.csv", index=False)
    else:
        print(df.head(10))


if __name__ == "__main__":
    main()
