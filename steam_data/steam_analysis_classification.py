from pprint import pprint

import matplotlib.font_manager as font_manager
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import seaborn as sns
from imblearn.combine import SMOTETomek
from matplotlib import rcParams
from sklearn import metrics, tree
from sklearn.ensemble import RandomForestClassifier
from sklearn.inspection import permutation_importance
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import (
    ConfusionMatrixDisplay,
    accuracy_score,
    confusion_matrix,
    mean_squared_error,
    precision_recall_fscore_support,
)
from sklearn.model_selection import GridSearchCV, train_test_split
from sklearn.preprocessing import (
    LabelEncoder,
    StandardScaler,
)
from sklearn.tree import DecisionTreeClassifier

font_dirs = ["/home/rhys/fonts"]  # The path to the custom font file.
font_files = font_manager.findSystemFonts(fontpaths=font_dirs)
for font_file in font_files:
    font_manager.fontManager.addfont(font_file)

print(font_manager.get_font_names())
# Set font family globally
rcParams["font.family"] = "Oxygen"

RS = 123


# Extract and sort feature coefficients
def get_feature_coefs(regression_model, label_index, columns):
    coef_dict = {}
    for coef, feat in zip(regression_model.coef_[0, :], columns):
        # for coef, feat in zip(regression_model.coef_[label_index, :], columns):
        if abs(coef) >= 0.01:
            coef_dict[feat] = coef
    # Sort coefficients
    coef_dict = {k: v for k, v in sorted(coef_dict.items(), key=lambda item: item[1])}
    return coef_dict


def get_accuracy(X_tr, X_t, y_tr, y_t, model):
    return {
        "test Accuracy": accuracy_score(y_t, model.predict(X_t)),
        "train Accuracy": accuracy_score(y_tr, model.predict(X_tr)),
    }


def plot_decision_tree(model, feature_names, plot_name):
    plt.subplots(figsize=(10, 6))
    tree.plot_tree(model, feature_names=feature_names, filled=True, max_depth=3)
    plt.savefig(f"./plots/dt_{plot_name}.png", dpi=300)


# Generate bar colors based on if value is negative or positive
def get_bar_colors(values):
    color_vals = []
    for val in values:
        if val <= 0:
            color_vals.append("xkcd:light red")
        else:
            color_vals.append("xkcd:light green")
    return color_vals


# Visualize coefficients
def visualize_coefs(coef_dict, save_path):
    features = list(coef_dict.keys())
    values = list(coef_dict.values())
    y_pos = np.arange(len(features))
    color_vals = get_bar_colors(values)
    plt.rcdefaults()
    fig, ax = plt.subplots(figsize=(8, 8))
    ax.barh(y_pos, values, align="center", color=color_vals)
    ax.set_yticks(y_pos)
    ax.set_yticklabels(features)
    # labels read top-to-bottom
    ax.invert_yaxis()
    ax.set_xlabel("Feature Coefficients")
    ax.set_title("Logistic Regression")
    fig.savefig(f"{save_path}", bbox_inches="tight")
    plt.close()


def evaluate_metrics(yt, yp):
    results_pos = {}
    results_pos["accuracy"] = accuracy_score(yt, yp)
    precision, recall, f_beta, _ = precision_recall_fscore_support(
        yt, yp, average="binary"
    )
    results_pos["recall"] = recall
    results_pos["precision"] = precision
    results_pos["f1score"] = f_beta
    return results_pos


def RMSE(ytrue, ypredicted):
    return np.sqrt(mean_squared_error(ytrue, ypredicted))


def skew_check(data, skew_limit):
    skew_vals = data.skew()
    skew_cols = (
        skew_vals.sort_values(ascending=False)
        .to_frame()
        .rename(columns={0: "Skew"})
        .query(f"abs(Skew) > {skew_limit}")
    )
    return skew_cols


def plot_value_counts(y, title, label_encoder=None):
    if label_encoder:
        y = pd.Series(label_encoder.inverse_transform(y.values.ravel()))

    fig, ax = plt.subplots(figsize=(8, 5))
    y.value_counts().plot.barh(ax=ax, color=["xkcd:teal", "xkcd:amber", "xkcd:purple"])
    ax.set(xlabel="Value Count", ylabel="Review Class", title=title)
    fig.savefig(
        f"./plots/{title.lower().replace(' ', '_')}.png", dpi=300, bbox_inches="tight"
    )
    plt.close()


def log_regr(X_train, X_test, y_train, y_test, le):
    print("\n\nLog. Regr ============================================================")
    # Max iteration = 1000
    max_iter = 1000

    model = LogisticRegression(max_iter=max_iter)

    parameters = [
        {
            "penalty": ["elasticnet"],
            "solver": ["saga"],
            "C": [0.001, 0.01, 0.1, 1, 10, 100],
            "l1_ratio": np.arange(0, 1.05, step=0.1),
        },
        {
            "penalty": ["l1"],
            "solver": ["saga"],
            "C": [0.001, 0.01, 0.1, 1, 10, 100],
        },
        {
            "penalty": ["l2"],
            "solver": ["lbfgs", "liblinear", "sag", "saga", "newton-cg"],
            "C": [0.001, 0.01, 0.1, 1, 10, 100],
        },
    ]

    grid_search = GridSearchCV(
        estimator=model, param_grid=parameters, scoring="f1", cv=5, verbose=True
    )

    grid_search.fit(X_train, y_train)

    print("\n Fitting best params -> cross validation")
    print(grid_search.best_params_)
    print(f"with best accuracy F1 = {grid_search.best_score_:.3f}")

    pprint(get_accuracy(X_train, X_test, y_train, y_test, grid_search.best_estimator_))

    custom_model = grid_search.best_estimator_
    custom_model.fit(X_train, y_train)
    preds = custom_model.predict(X_test)

    pprint(evaluate_metrics(y_test, preds))

    # Confusion Matrix ====================================================
    cf = confusion_matrix(y_test, preds, normalize="true")
    sns.set_context("talk")
    disp = ConfusionMatrixDisplay(
        confusion_matrix=cf, display_labels=le.inverse_transform(custom_model.classes_)
    )
    disp.plot()
    plt.savefig(f"plots/log_res_conf_matrix.png", bbox_inches="tight")
    plt.close()

    # Interpretation ======================================================
    coef_dict = get_feature_coefs(custom_model, 0, FEATURES)
    visualize_coefs(coef_dict, f"./plots/log_res_coeffs.png")


def dec_trees(X_train, X_test, y_train, y_test):
    print("\n\nDec. Tree ============================================================")
    params_grid = {
        "criterion": ["gini", "entropy"],
        "max_depth": range(2, 10 + 1, 2),
        "min_samples_leaf": range(2, 20 + 1, 2),
        "min_samples_split": range(2, 20 + 1, 2),
    }
    model = DecisionTreeClassifier(random_state=RS)

    grid_search = GridSearchCV(
        estimator=model, param_grid=params_grid, scoring="f1", cv=5, verbose=1
    )
    grid_search.fit(X_train, y_train)
    print("\n Fitting best params -> cross validation")
    print(grid_search.best_params_)
    print(f"with best accuracy F1 = {grid_search.best_score_:.3f}")

    pprint(get_accuracy(X_train, X_test, y_train, y_test, grid_search.best_estimator_))

    custom_model = grid_search.best_estimator_
    custom_model.fit(X_train, y_train)
    preds = custom_model.predict(X_test)
    print('\n "Best" Tree')
    pprint(evaluate_metrics(y_test, preds))
    # Plot the decision tree
    plot_decision_tree(custom_model, FEATURES, "cross-validated")


def random_forest(X_train, X_test, y_train, y_test):
    print("\n\nRandom Forest ========================================================")
    model = RandomForestClassifier()

    # print(model.get_params().keys())
    param_grid = {
        "n_estimators": [2 * n + 1 for n in range(20)],
        "max_depth": [2 * n + 1 for n in range(10)],
        "max_features": ["sqrt", "log2"],
    }
    grid_search = GridSearchCV(
        estimator=model, param_grid=param_grid, scoring="f1", cv=5
    )
    grid_search.fit(X_train, y_train)
    print("\n Fitting best params -> cross validation")
    print(grid_search.best_params_)
    print(f"with best accuracy F1 = {grid_search.best_score_:.3f}")

    pprint(get_accuracy(X_train, X_test, y_train, y_test, grid_search.best_estimator_))

    custom_model = grid_search.best_estimator_
    custom_model.fit(X_train, y_train)
    preds = custom_model.predict(X_test)
    pprint(evaluate_metrics(y_test, preds))

    # Use permutation_importance to calculate permutation feature importances
    pi = permutation_importance(
        estimator=grid_search.best_estimator_,
        X=X_train,
        y=y_train,
        n_repeats=10,
        random_state=RS,
        n_jobs=2,
    )
    sorted_pi = pi.importances_mean.argsort()[::-1]

    print("\nFeature Importance:")

    fig, ax = plt.subplots(figsize=(10, 9))
    ax.boxplot(
        pi.importances[sorted_pi].T,
        orientation="horizontal",
        labels=[FEATURES[i] for i in sorted_pi],
    )
    ax.set_title("Permutation Importances (test set)")
    fig.tight_layout()
    fig.savefig(f"./plots/random_forest_import.png", dpi=300, bbox_inches="tight")
    plt.close()

    for i in sorted_pi:
        if pi.importances_mean[i] - 2 * pi.importances_std[i] > 0:
            print(
                f"{FEATURES[i]:<8}"
                f"{pi.importances_mean[i]:.3f}"
                f" +/- {pi.importances_std[i]:.3f}"
            )


def clean_data(data, target):
    print("\n\nData Cleaning =========================================================")
    # Find missing values
    print(data.isnull().sum())
    # Look at unique app_id column for duplicated entries
    print(sum(data.duplicated(subset="app_id")) == 0)

    print("\nGenres:")
    print(data.genre.value_counts())

    # Reassign values with low population to other
    other_threshold = 20
    to_other = data.genre.value_counts().loc[lambda x: x <= other_threshold].index
    data["genre"] = data.genre.replace(to_other, "Other")
    # Validate choice of other_threshold
    print("\n Final genres:")
    print(data.genre.value_counts())

    # Drop columns that will no longer be needed for the analysis
    data.drop(["app_id", "name"], axis=1, inplace=True)

    # One-hot encoding for genre column -> only column with categorical data
    data = pd.get_dummies(data, columns=["genre"], dtype=float)

    # Confirm that the data cleaning and one-hot encoding removed all text columnns
    data.select_dtypes(include=["object"]).columns.empty

    float_cols = ["price", "recommendations"]

    sk = skew_check(data[float_cols], 0.75)
    print(sk)

    for col in sk.index.tolist():
        if col == target:
            continue
        data[col] = data[col].apply(np.log1p)

    print(data[float_cols].skew())

    X = data.drop(target, axis=1)
    y = data[target]

    return X, y


def main() -> None:
    data = pd.read_csv("./data/steam_data_clean_cls.csv")

    # Data Cleaning ===============================================================

    n_classes: int = 3

    X, y = clean_data(data, target="review_class")
    global FEATURES
    FEATURES = list(X.columns)

    s = StandardScaler()
    X_ss = s.fit_transform(X)

    # Create a LabelEncoder object
    le = LabelEncoder()
    # Encode the target variable
    y = pd.Series(le.fit_transform(y.values.ravel()))

    X_raw, X_test, y_raw, y_test = train_test_split(
        X_ss, y, test_size=0.2, stratify=y, random_state=1809
    )

    # Class balance ===============================================================

    plot_value_counts(y_raw, "Class Balance Check", le)

    smote_tomek = SMOTETomek(random_state=0)
    X_train, y_train = smote_tomek.fit_resample(X_raw, y_raw)

    plot_value_counts(y_train, "Resampled Class Balance Check", le)

    # Logostic Regression =========================================================
    log_regr(X_train, X_test, y_train, y_test, le)
    # Decision Trees ==============================================================
    dec_trees(X_train, X_test, y_train, y_test)
    # Random Forest ===============================================================
    random_forest(X_train, X_test, y_train, y_test)


if __name__ == "__main__":
    main()
