import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
from sklearn.linear_model import (
    ElasticNetCV,
    LassoCV,
    LinearRegression,
    RidgeCV,
)
from sklearn.metrics import mean_squared_error, r2_score
from sklearn.model_selection import train_test_split
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler


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


data = pd.read_csv("./data/steam_data_clean.csv")


# Data Summary ================================================================
# The data in this analysis is taken from the Steam Store (Oct.
# 2024) as found in the larger data set at
# https://github.com/NewbieIndieGameDev/steam-insights , combining data from
# the Valve and SteamSpy APIs. The data has been pre-cleaned and reduced
# signicicantly, with the top 2000 most highly played/downloaded games being
# selected.
#
# - data size = 2000 observations, 13 features (1 categorical, 12 numerical) -
# key variables: - price = price of the game now - recommendations = number of
# recommendations of game on Steam - est_users = estimated total downloads from
# the Steam Store - release_year = year of release - genre = the approximate
# genre of the game (data cleaned significantly) - has_(feature) = whether or
# not the game has a feature, chosen from a subset of available features. -
# target variable: - review_perc = percentage of total reviews that were
# positive (>95% = Overwhelmingly Positive, etc.)

# show the first 5 rows using dataframe.head() method
# print("The first 5 rows:")
# print(data.head(5))
# print(data.info())
# print(data.describe())
# print(data.shape)

# Data Cleaning ===============================================================

# Find missing values
print(data.isnull().sum())
# Look at unique app_id column for duplicated entries
print(sum(data.duplicated(subset="app_id")) == 0)

print(data.genre.value_counts())

# Reassign values with low population to other
other_threshold = 20
to_other = data.genre.value_counts().loc[lambda x: x <= other_threshold].index
data["genre"] = data.genre.replace(to_other, "Other")
# Validate choice of threshold
print(data.genre.value_counts())

# Drop columns that will no longer be needed for the analysis
data.drop(["app_id", "name"], axis=1, inplace=True)

# One-hot encoding for genre column -> only column with categorical data
data = pd.get_dummies(data, columns=["genre"], dtype=float)

# Confirm that the data cleaning and one-hot encoding removed all text columnns
data.select_dtypes(include=["object"]).columns.empty

TARGET = "review_perc"

float_cols = ["price", "recommendations"]

sk = skew_check(data[float_cols], 0.75)
print(sk)

for col in sk.index.tolist():
    if col == TARGET:
        continue
    data[col] = data[col].apply(np.log1p)

print(data[float_cols].skew())


# Objective of the Analysis
#
# The objective of the analysis is to investigate which of the features has the
# biggest impact on the target variable, and in which direction. That is, which
# factors contribute the most to the reception of a game and the opinion of
# reviewers.

# Analysis

X = data.drop(TARGET, axis=1)
y = data[TARGET]

# pf = PolynomialFeatures(degree=2, include_bias=False,)
# X_pf = pf.fit_transform(X)

s = StandardScaler()
X_ss = s.fit_transform(X)

# lr_unscaled = LinearRegression()
# lr_unscaled.fit(X, y)
# print(lr_unscaled.coef_)

lr = LinearRegression()
lr.fit(X_ss, y)
# coefficients now "on the same scale"
print(lr.coef_)

# Find top 5 most impactful, and in which "direction"
print(
    pd.DataFrame(zip(X.columns, lr.coef_))
    .sort_values(by=1, key=abs, ascending=False)
    .head(5)
)

X_train, X_test, y_train, y_test = train_test_split(
    X_ss, y, test_size=0.3, random_state=1809
)

# Linear Regression ===========================================================
lin_reg = LinearRegression().fit(X_train, y_train)
y_lr = lin_reg.predict(X_test)

lin_reg_rmse = RMSE(y_test, y_lr)
lin_reg_r2 = r2_score(y_test, y_lr)

fig = plt.figure(figsize=(6, 6))
ax = plt.axes()
ax.plot(y_test, y_lr, marker="o", ls="", ms=3.0)
lim = (np.minimum(y_test, y_lr).min() - 5, np.maximum(y_test, y_lr).max() + 5)
ax.set(
    xlabel="Actual % Positive",
    ylabel="Predicted % Positive",
    xlim=lim,
    ylim=lim,
    title="Linear Regression Results",
)
fig.text(0.7, 0.3, f"RMSE {lin_reg_rmse:.2f}")
fig.text(0.7, 0.25, f"$R^2$ {lin_reg_r2:.2f}")


# Ridge Regression ========================================================
# alphas to test the model
alphas = np.array([600, 650, 700, 725, 750, 775, 800, 900])
# cross validation of the Ridge model
ridgeCV = RidgeCV(alphas=alphas, cv=3).fit(X_train, y_train)
y_ridge = ridgeCV.predict(X_test)


ridge_rmse = RMSE(y_test, y_ridge)
ridge_r2 = r2_score(y_test, y_ridge)

print(f"{'Ridge':10} -> Best Alpha: {ridgeCV.alpha_} | RMSE: {ridge_rmse:.1f}")

fig = plt.figure(figsize=(6, 6))
ax = plt.axes()
ax.plot(y_test, y_ridge, marker="o", ls="", ms=3.0)
lim = (np.minimum(y_test, y_ridge).min() - 5, np.maximum(y_test, y_ridge).max() + 5)
ax.set(
    xlabel="Actual % Positive",
    ylabel="Predicted % Positive",
    xlim=lim,
    ylim=lim,
    title="Ridge Results",
)
fig.text(0.7, 0.3, f"RMSE {ridge_rmse:.2f}")
fig.text(0.7, 0.25, f"$R^2$ {ridge_r2:.2f}")


# Lasso Regression ========================================================
alphas = np.array([0.05, 0.1, 0.25, 0.5, 0.75, 1, 5])

lassoCV = LassoCV(alphas=alphas, max_iter=int(10e4), cv=3).fit(X_train, y_train)
y_lasso = lassoCV.predict(X_test)

lasso_rmse = RMSE(y_test, y_lasso)
lasso_r2 = r2_score(y_test, y_lasso)

print(f"{'Lasso':10} -> Best Alpha: {lassoCV.alpha_} | RMSE: {lasso_rmse:.1f}")

fig = plt.figure(figsize=(6, 6))
ax = plt.axes()
ax.plot(y_test, y_lasso, marker="o", ls="", ms=3.0)
lim = (np.minimum(y_test, y_lasso).min() - 5, np.maximum(y_test, y_lasso).max() + 5)
ax.set(
    xlabel="Actual % Positive",
    ylabel="Predicted % Positive",
    xlim=lim,
    ylim=lim,
    title="Lasso Results",
)
fig.text(0.7, 0.3, f"RMSE {lasso_rmse:.2f}")
fig.text(0.7, 0.25, f"$R^2$ {lasso_r2:.2f}")
lassoCV_rmse = RMSE(y_test, lassoCV.predict(X_test))

print(
    f"{'Lasso':10} -> Of {len(lassoCV.coef_)} coeffs, {len(lassoCV.coef_.nonzero()[0])} are non-zero."
)

# Elastic Net =============================================================
# Rations of Lasso : Ridge
alphas = np.array([0.05, 0.075, 0.1, 0.125, 0.15, 0.2])
l1_ratios = np.linspace(0.1, 0.9, 9)
enetCV = ElasticNetCV(alphas=alphas, l1_ratio=l1_ratios, max_iter=int(1e4)).fit(
    X_train, y_train
)

y_enet = enetCV.predict(X_test)

enet_rmse = RMSE(y_test, y_enet)
enet_r2 = r2_score(y_test, y_enet)

print(f"{'ENet':10} -> Best Alpha: {enetCV.alpha_} | RMSE: {enet_rmse:.1f}")
print(
    f"{'Enet':10} -> Of {len(enetCV.coef_)} coeffs, {len(enetCV.coef_.nonzero()[0])} are non-zero."
)

fig = plt.figure(figsize=(6, 6))
ax = plt.axes()
ax.plot(y_test, y_enet, marker="o", ls="", ms=3.0)
lim = (np.minimum(y_test, y_enet).min() - 5, np.maximum(y_test, y_enet).max() + 5)
ax.set(
    xlabel="Actual % Positive",
    ylabel="Predicted % Positive",
    xlim=lim,
    ylim=lim,
    title="ENet Results",
)
fig.text(0.7, 0.3, f"RMSE {enet_rmse:.2f}")
fig.text(0.7, 0.25, f"$R^2$ {enet_r2:.2f}")


# ## Model Comparison
#
# From the two comparisons below, the Ridge regression using a best alpha value
# of 600 was the most effective at predicting the target variable on the
# hold-out test data. The Ridge results had both a lower RMSE and the highest
# R2 score out of all of the tested models. From a visual inspection of the
# predicted values, it is possible to see that the performance is very close
# between all of the models. Combined with the high RMSEs and low R2 scores
# overall, it is clear that this data set is hard to model with any definitive
# level of accuracy, using these features.

# Comparison of methods ===================================================
# Compare RMSE in a simple table
rmse_vals = [lin_reg_rmse, ridge_rmse, lasso_rmse, enet_rmse]
r2_vals = [lin_reg_r2, ridge_r2, lasso_r2, enet_r2]
labels = ["Linear", "Ridge", "Lasso", "ElasticNet"]
res_df = pd.Series(rmse_vals, index=labels).to_frame()
res_df["R2"] = r2_vals
res_df.rename(columns={0: "RMSE"}, inplace=1)
print("\nRMSE Results for each regression:")
print(res_df)

# Plot the actual vs. predicted for all models
fig = plt.figure(figsize=(6, 6))
ax = plt.axes()
labels = ["Ridge", "Lasso", "ElasticNet"]
models = [ridgeCV, lassoCV, enetCV]
for mod, lab in zip(models, labels):
    ax.plot(y_test, mod.predict(X_test), marker="o", ls="", ms=3.0, label=lab)

j = np.linspace(y_test.min(), y_test.max(), 100)
ax.plot(j, j, c="k", alpha=0.5, ls="--")
leg = plt.legend(frameon=True)
leg.get_frame().set_edgecolor("black")
leg.get_frame().set_linewidth(1.0)
ax.set(
    xlabel="Actual % Positive",
    ylabel="Predicted % Positive",
    title="Comparison of Regression Results",
)


# ## Key Findings
#
# The tables below highlight the key findings of the investigation.
#
# The most insightful metric when predicting review scores was the number of
# recommendations that a game recieved. This is not surprising, as both metrics
# are public opinion and could potentially considered correlated.
#
# The existence of In-App Purchases within a game was also found to be the most
# significant factors negatively impacting the review score. This shows how the
# community has responded to recent trends and monetisation practices within
# the industry.
#
# Another negatively influencing feature was the year in which a game was
# released, meaning that more recent games have been getting poorer reviews
# overall than older games. Potentially this will be biased by the fact that
# there are far more games now than in the past, by interesting to note
# nonetheless.
#
#
# ### Genre Analysis Looking more closely at which genres performed best, and
# worse within the data set, it was found that: Indie, Simulation and Casual
# games are the most likely to be positively rated, whilst Racing and Action
# games are the least favoured by players. The fact that Early Access, whilst
# not necessarily a genre but a game state, is the most negatively impacting
# tag within the data, highlights the community reaction to the current
# practices within the industry.

# get the final results from the ENet
final = (
    pd.DataFrame(zip(X.columns, ridgeCV.coef_))
    .sort_values(by=1, key=abs, ascending=False)
    .rename(columns={0: "feature", 1: "significance"})
)

# Find top 5 most impactful, and in which "direction"
final.head(5)
# Top 3 most negative genres
final[final["feature"].str.contains("genre_")].sort_values(by="significance").head(3)
# Top 3 most positive genres
final[final["feature"].str.contains("genre_")].sort_values(
    by="significance", ascending=False
).head(3)


# ## Limitations and Next Steps Some potential limitations of the data set and
# analysis, and how one might improve them, are as follows: 1. Price was not
# currency corrected, which may have influenced the result as Yen vs. Euros is
# not a reliable or comparible metric. 2. The genre tags were potentially not
# "correct" or complete as many games fit into multiple genres and will be
# tagged as such. 3. The final results clearly show that the models do not
# predict the target variable in a satisfactory way. This is shown in the
# comparison of the RMSE and R2 scores for all the models.
#
# In order to improve the performance of the models, a number of steps can be
# taken. Adding more features may improve the results as it may find
# descriptors that perform even better than the ones selected for this
# analysis. Furthermore, adding more observations to better improve the data
# available to the model, as the target variable is skewed towards the positive
# end of the review % due to the filtering outside of the analysis. Filling in
# the lower end may allow for a better modelling overall.
#
# Finally, the use of a different modelling technique such as Stochastic
# Gradient Descent (SGD) could help to improve the final results. This could
# lead to a more effective learning rate and be an enhancement over the Elastic
# Net cross validation currently used.
